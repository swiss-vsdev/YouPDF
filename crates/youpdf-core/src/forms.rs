//! Formulaires AcroForm.
//!
//! ## Support
//!
//! - **Lecture** : liste des champs avec nom, type, valeur actuelle,
//!   options (pour les listes et boutons radio).
//! - **Remplissage** : modification de la valeur `/V` d'un champ,
//!   plus `/AS` (appearance state) pour les cases à cocher.
//! - **Aplatissement** : transformation des champs en contenu de page
//!   non modifiable (prévu phase 5).
//!
//! ## Types de champ supportés
//!
//! | Type PDF     | Signification          |
//! |--------------|------------------------|
//! | `/Tx`        | Champ texte            |
//! | `/Btn`       | Case à cocher / radio  |
//! | `/Ch`        | Liste déroulante       |
//! | `/Sig`       | Champ de signature     |

use lopdf::{Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::util::{csv_escape, escape_xml, unescape_xml};

/// Type d'un champ de formulaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormFieldKind {
    /// Champ texte.
    Text,
    /// Case à cocher.
    Checkbox,
    /// Bouton radio.
    Radio,
    /// Liste déroulante (choix unique).
    Choice,
    /// Liste multi-sélection.
    MultiChoice,
    /// Champ de signature.
    Signature,
    /// Type inconnu ou non supporté.
    Other(String),
}

impl FormFieldKind {
    /// Traduit un code `/FT` en type utilisable.
    fn from_ft(ft: &[u8]) -> Self {
        match ft {
            b"Tx" => FormFieldKind::Text,
            b"Ch" => FormFieldKind::Choice,
            b"Sig" => FormFieldKind::Signature,
            b"Btn" => FormFieldKind::Checkbox, // affiné plus tard
            other => FormFieldKind::Other(String::from_utf8_lossy(other).to_string()),
        }
    }

    /// Nom lisible pour l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            FormFieldKind::Text => "Texte",
            FormFieldKind::Checkbox => "Case à cocher",
            FormFieldKind::Radio => "Bouton radio",
            FormFieldKind::Choice => "Liste",
            FormFieldKind::MultiChoice => "Liste multiple",
            FormFieldKind::Signature => "Signature",
            FormFieldKind::Other(_) => "Autre",
        }
    }
}

/// Un champ de formulaire lu depuis un PDF.
#[derive(Debug, Clone)]
pub struct FormField {
    /// Référence de l'objet PDF (pour la mise à jour).
    pub object_id: (u32, u16),
    /// Nom complet (peut être hiérarchique : `parent.enfant`).
    pub name: String,
    /// Type.
    pub kind: FormFieldKind,
    /// Valeur actuelle (chaîne pour texte, code pour choix).
    pub value: Option<String>,
    /// Options possibles (pour listes et boutons radio).
    pub options: Vec<String>,
    /// Vrai si le champ est en lecture seule.
    pub read_only: bool,
    /// Aide affichée au survol (`/TU`).
    pub tooltip: String,
    /// Champ obligatoire (bit 2 de `/Ff`).
    pub required: bool,
    /// Champ multiligne (bit 13 de `/Ff`, champs texte).
    pub multiline: bool,
    /// Numéro de page (1-indexé), si le champ en a une.
    pub page: Option<usize>,
    /// Rectangle du champ en repère page : `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
}

/// Description d'un champ de formulaire à créer.
///
/// Les coordonnées sont en repère PDF (origine en bas à gauche), comme
/// partout ailleurs dans le cœur : `[x1, y1, x2, y2]`.
#[derive(Debug, Clone)]
pub struct NewFormField {
    /// Nom du champ (`/T`), tel qu'il apparaît dans le panneau.
    pub name: String,
    /// Type du champ.
    pub kind: FormFieldKind,
    /// Numéro de page (1-indexé).
    pub page: u32,
    /// Rectangle du champ.
    pub rect: [f32; 4],
    /// Choix proposés (listes déroulantes seulement).
    pub options: Vec<String>,
    /// Valeur initiale : texte saisi, ou état coché pour une case.
    pub value: Option<String>,
    /// Champ texte multiligne.
    pub multiline: bool,
    /// Champ en lecture seule.
    pub read_only: bool,
}

impl NewFormField {
    /// Champ texte d'une ligne.
    pub fn text(page: u32, rect: [f32; 4], name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: FormFieldKind::Text,
            page,
            rect,
            options: Vec::new(),
            value: None,
            multiline: false,
            read_only: false,
        }
    }

    /// Case à cocher.
    pub fn checkbox(page: u32, rect: [f32; 4], name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: FormFieldKind::Checkbox,
            page,
            rect,
            options: Vec::new(),
            value: None,
            multiline: false,
            read_only: false,
        }
    }

    /// Liste déroulante.
    pub fn choice(page: u32, rect: [f32; 4], name: impl Into<String>, options: &[&str]) -> Self {
        Self {
            name: name.into(),
            kind: FormFieldKind::Choice,
            page,
            rect,
            options: options.iter().map(|o| (*o).to_string()).collect(),
            value: None,
            multiline: false,
            read_only: false,
        }
    }
}

impl Document {
    /// Lit tous les champs de formulaire du document.
    pub fn read_form_fields(&self) -> Vec<FormField> {
        let mut out = Vec::new();

        // 1. Récupérer le catalogue → /AcroForm.
        let Some(acroform) = self.acroform_dict() else {
            return out;
        };
        // `/Fields` peut être un tableau direct ou une référence : les
        // deux formes sont valides, et beaucoup de producteurs choisissent
        // la seconde.
        let fields: Vec<Object> = match acroform.get(b"Fields") {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_object(*id)
                .ok()
                .and_then(|obj| obj.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => return out,
        };

        // 2. Parcourir récursivement (les champs peuvent être hiérarchiques).
        for field_ref in &fields {
            let Ok(field_id) = field_ref.as_reference() else {
                continue;
            };
            self.walk_field(field_id, None, &mut out);
        }

        out
    }

    /// Parcourt un champ et ses enfants.
    fn walk_field(
        &self,
        field_id: (u32, u16),
        parent_name: Option<&str>,
        out: &mut Vec<FormField>,
    ) {
        let Ok(obj) = self.inner().get_object(field_id) else {
            return;
        };
        let Ok(dict) = obj.as_dict() else {
            return;
        };

        // Nom local du champ. Les noms sont encodés comme le reste : WinAnsi
        // ou UTF-16BE, pas forcément de l'UTF-8.
        let local_name = dict
            .get(b"T")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(crate::structure::decode_pdf_text);

        let full_name = match (parent_name, &local_name) {
            (Some(p), Some(n)) => format!("{p}.{n}"),
            (None, Some(n)) => n.clone(),
            (Some(p), None) => p.to_string(),
            (None, None) => String::new(),
        };

        // Un champ feuille a un /FT (ou hérite de son parent — simplification :
        // on considère qu'il a un /FT direct).
        if let Ok(ft) = dict.get(b"FT").and_then(|o| o.as_name()) {
            let mut kind = FormFieldKind::from_ft(ft);

            // Affiner pour /Btn : radio ou checkbox.
            if matches!(kind, FormFieldKind::Checkbox) {
                let flags = dict
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                // Bit 15 (0x8000) : radio.
                // Bit 16 (0x10000) : pushbutton (à ignorer).
                if flags & 0x8000 != 0 {
                    kind = FormFieldKind::Radio;
                } else if flags & 0x10000 != 0 {
                    kind = FormFieldKind::Other("Bouton".to_string());
                }
            }

            // Affiner pour /Ch : multi-sélection ?
            if matches!(kind, FormFieldKind::Choice) {
                let flags = dict
                    .get(b"Ff")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0);
                // Bit 21 : MultiSelect (le bit 22 est DoNotSpellCheck).
                if flags & (1 << 20) != 0 {
                    kind = FormFieldKind::MultiChoice;
                }
            }

            // Valeur actuelle. Les valeurs des champs sont encodées en
            // **WinAnsi** (la page de codes de la police standard) : un
            // `String::from_utf8_lossy` remplaçait chaque octet ≥ 0x80 par le
            // caractère de remplacement, et un accent ressortait en point
            // d'interrogation. On décode donc comme le reste du texte PDF :
            // UTF-16BE, sinon UTF-8, sinon Latin-1.
            let value = dict.get(b"V").ok().and_then(|o| match o {
                Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(bytes)),
                Object::Name(n) => Some(crate::structure::decode_pdf_text(n)),
                _ => None,
            });

            // Options (pour les listes).
            let options = dict
                .get(b"Opt")
                .ok()
                .and_then(|o| o.as_array().ok())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| match opt {
                            Object::String(bytes, _) => {
                                Some(crate::structure::decode_pdf_text(bytes))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Flags lecture seule (bit 1), obligatoire (bit 2), multiligne
            // (bit 13).
            let flags = dict
                .get(b"Ff")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            let read_only = flags & 1 != 0;
            let required = flags & 2 != 0;
            let multiline = flags & 4096 != 0;

            // Aide au survol.
            let tooltip = dict
                .get(b"TU")
                .ok()
                .and_then(|o| o.as_str().ok())
                .map(crate::structure::decode_pdf_text)
                .unwrap_or_default();

            // Page du champ (via /P si présent).
            let page = dict
                .get(b"P")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|page_id| self.page_number_of(page_id));

            // Rectangle du champ.
            let rect = {
                dict.get(b"Rect")
                    .and_then(|o| o.as_array())
                    .and_then(|arr| {
                        let vals: Vec<f32> = arr
                            .iter()
                            .filter_map(|o| o.as_i64().ok())
                            .map(|v| v as f32)
                            .collect();
                        if vals.len() >= 4 {
                            Ok(Some([vals[0], vals[1], vals[2], vals[3]]))
                        } else {
                            Ok(None)
                        }
                    })
                    .unwrap_or(None)
                    .map(|r| r)
                    .unwrap_or([0.0, 0.0, 0.0, 0.0])
            };

            if !full_name.is_empty() {
                out.push(FormField {
                    object_id: field_id,
                    name: full_name.clone(),
                    kind,
                    value,
                    options,
                    read_only,
                    tooltip,
                    required,
                    multiline,
                    page,
                    rect,
                });
            }
        }

        // Enfants éventuels.
        if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
            for kid in kids {
                if let Ok(kid_id) = kid.as_reference() {
                    self.walk_field(kid_id, Some(&full_name), out);
                }
            }
        }
    }

    /// Remplit un champ de formulaire.
    ///
    /// Pour les cases à cocher et boutons radio, la valeur doit être un
    /// nom d'état (`/Off` par défaut, ou un nom présent dans `/AP`).
    /// On met à jour `/V` et `/AS` simultanément.
    pub fn fill_form_field(&mut self, field: &FormField, value: &str) -> Result<()> {
        if field.read_only {
            return Err(Error::InvalidInput(format!(
                "le champ « {} » est en lecture seule",
                field.name
            )));
        }

        // `set_field_value` met `/V` et, pour les cases, `/AS`.
        self.set_field_value(field, value)?;

        // Marquer le document comme ayant des changements (côté appelant).

        tracing::debug!(field = %field.name, "champ rempli");
        Ok(())
    }

    /// Crée un champ de formulaire et l'inscrit dans `/AcroForm`.
    ///
    /// Le champ reçoit une apparence (`/AP`) pour que les lecteurs qui
    /// n'ont pas de générateur d'apparences l'affichent quand même, et
    /// `/NeedAppearances` reste vrai pour ceux qui en ont un.
    pub fn create_form_field(&mut self, field: &NewFormField) -> Result<FormField> {
        if field.name.trim().is_empty() {
            return Err(Error::InvalidInput(
                "un champ de formulaire doit avoir un nom".to_string(),
            ));
        }
        let [x1, y1, x2, y2] = field.rect;
        let rect = [
            x1.min(x2),
            y1.min(y2),
            x1.max(x2),
            y1.max(y2),
        ];
        let w = (rect[2] - rect[0]).max(4.0);
        let h = (rect[3] - rect[1]).max(4.0);

        let page_id = self
            .page_id(field.page)
            .map_err(|e| Error::Pdf(format!("page {} : {e}", field.page)))?;
        let font = self.ensure_appearance_font()?;

        // Flags de champ (`/Ff`).
        let mut flags: i64 = match field.kind {
            FormFieldKind::Text => {
                if field.multiline {
                    1 << 12
                } else {
                    0
                }
            }
            FormFieldKind::Radio => 1 << 15,
            FormFieldKind::Choice => 1 << 17,
            FormFieldKind::MultiChoice => (1 << 17) | (1 << 20),
            _ => 0,
        };
        if field.read_only {
            // Bit 1 : lecture seule.
            flags |= 1;
        }

        let ft: &[u8] = match field.kind {
            FormFieldKind::Text => b"Tx",
            FormFieldKind::Checkbox | FormFieldKind::Radio => b"Btn",
            FormFieldKind::Choice | FormFieldKind::MultiChoice => b"Ch",
            FormFieldKind::Signature => b"Sig",
            FormFieldKind::Other(ref other) => {
                return Err(Error::InvalidInput(format!(
                    "type de champ non créable : {other}"
                )))
            }
        };

        // Apparence : un XObject formulaire par état. Une case a deux
        // états (`/Off` et `/Yes`), les autres un seul.
        let font_obj = Object::Reference(font);
        let (ap_obj, is_choice_state) = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => {
                let off = self.add_appearance(font_obj.clone(), w, h, None, false)?;
                let on = self.add_appearance(font_obj.clone(), w, h, None, true)?;
                let mut states = Dictionary::new();
                states.set("Off", Object::Reference(off));
                states.set("Yes", Object::Reference(on));
                (
                    Object::Dictionary(states),
                    true,
                )
            }
            _ => {
                let value = field.value.as_deref();
                let id = self.add_appearance(font_obj.clone(), w, h, value, false)?;
                (Object::Reference(id), false)
            }
        };

        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget.set("FT", Object::Name(ft.to_vec()));
        widget.set(
            "T",
            Object::string_literal(crate::edit::encode_pdf_literal(&field.name)),
        );
        widget.set(
            "Rect",
            Object::Array(vec![
                Object::Real(rect[0]),
                Object::Real(rect[1]),
                Object::Real(rect[2]),
                Object::Real(rect[3]),
            ]),
        );
        widget.set("P", Object::Reference(page_id));
        widget.set("F", Object::Integer(4));
        if flags != 0 {
            widget.set("Ff", Object::Integer(flags));
        }
        widget.set("DA", Object::string_literal(b"/Helv 0 Tf 0 g".to_vec()));
        let mut mk = Dictionary::new();
        mk.set(
            "BG",
            Object::Array(vec![Object::Real(1.0), Object::Real(1.0), Object::Real(1.0)]),
        );
        mk.set(
            "BC",
            Object::Array(vec![Object::Real(0.5), Object::Real(0.5), Object::Real(0.5)]),
        );
        widget.set("MK", Object::Dictionary(mk));
        if is_choice_state {
            let state = field.value.clone().unwrap_or_else(|| "Off".to_string());
            widget.set("V", Object::Name(state.as_bytes().to_vec()));
            widget.set("AS", Object::Name(state.as_bytes().to_vec()));
        } else if let Some(value) = field.value.as_deref() {
            widget.set(
                "V",
                Object::string_literal(crate::edit::encode_pdf_literal(value)),
            );
        }
        if !field.options.is_empty() {
            let opts: Vec<Object> = field
                .options
                .iter()
                .map(|o| Object::string_literal(crate::edit::encode_pdf_literal(o)))
                .collect();
            widget.set("Opt", Object::Array(opts));
        }
        let mut ap = Dictionary::new();
        ap.set("N", ap_obj);
        widget.set("AP", Object::Dictionary(ap));

        let widget_id = self.inner_mut().add_object(Object::Dictionary(widget));

        // Page : le widget rejoint les annotations de la page.
        let mut annots = self.page_annots(page_id);
        annots.push(Object::Reference(widget_id));
        self.set_page_annots(page_id, &annots)?;

        self.register_acroform_field(widget_id, font)?;
        self.touch();

        Ok(FormField {
            object_id: widget_id,
            name: field.name.clone(),
            kind: field.kind.clone(),
            value: field.value.clone(),
            options: field.options.clone(),
            read_only: field.read_only,
            tooltip: String::new(),
            required: false,
            multiline: false,
            page: Some(field.page as usize),
            rect: [0.0, 0.0, 0.0, 0.0],
        })
    }

    /// Ajoute un XObject d'apparence (fond blanc, cadre, croix ou texte).
    fn add_appearance(
        &mut self,
        font: Object,
        w: f32,
        h: f32,
        value: Option<&str>,
        checked: bool,
    ) -> Result<ObjectId> {
        // Fond et cadre, puis croix ou valeur selon le cas.
        let mut ops: Vec<u8> = Vec::new();
        ops.extend_from_slice(format!("1 1 1 rg 0 0 {w:.2} {h:.2} re f\n").as_bytes());
        ops.extend_from_slice(
            format!(
                "0.5 0.5 0.5 RG 0.7 w 0.35 0.35 {:.2} {:.2} re S\n",
                (w - 0.7).max(0.1),
                (h - 0.7).max(0.1)
            )
            .as_bytes(),
        );
        if checked {
            ops.extend_from_slice(b"0 0 0 RG 1.2 w\n");
            ops.extend_from_slice(
                format!("1.5 1.5 m {:.2} {:.2} l S\n", w - 1.5, h - 1.5).as_bytes(),
            );
            ops.extend_from_slice(
                format!("1.5 {:.2} m {:.2} 1.5 l S\n", h - 1.5, w - 1.5).as_bytes(),
            );
        }
        if let Some(value) = value {
            if !value.is_empty() {
                let size = (h * 0.6).clamp(6.0, 12.0);
                ops.extend_from_slice(format!("BT /Helv {size:.1} Tf 0 g 2 2 Td ").as_bytes());
                crate::annotate::push_text_literal(&mut ops, value);
                ops.extend_from_slice(b" Tj ET\n");
            }
        }

        let mut xobject = Dictionary::new();
        xobject.set("Type", Object::Name(b"XObject".to_vec()));
        xobject.set("Subtype", Object::Name(b"Form".to_vec()));
        xobject.set("FormType", Object::Integer(1));
        xobject.set(
            "BBox",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w),
                Object::Real(h),
            ]),
        );
        let mut fonts = Dictionary::new();
        fonts.set("Helv", font);
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        xobject.set("Resources", Object::Dictionary(resources));
        Ok(self.inner_mut().add_object(Stream::new(xobject, ops)))
    }

    /// Inscrit le widget dans `/Fields` de `/AcroForm`, en créant le
    /// dictionnaire s'il n'existe pas encore.
    fn register_acroform_field(&mut self, widget_id: ObjectId, font: ObjectId) -> Result<()> {
        let root_id = self
            .inner()
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| Error::Pdf("catalogue absent".to_string()))?;

        // Base : le dictionnaire existant, cloné pour éviter les emprunts
        // croisés pendant la réécriture.
        let base = self.acroform_dict().cloned();
        let mut acro = base.unwrap_or_default();
        // Un `/Fields` indirect se modifie sur place : le remplacer par un
        // tableau neuf ferait disparaître les champs déjà déclarés.
        match acro.get(b"Fields").cloned() {
            Ok(Object::Reference(array_id)) => {
                if let Ok(Object::Array(array)) = self.inner_mut().get_object_mut(array_id) {
                    array.push(Object::Reference(widget_id));
                }
            }
            Ok(Object::Array(mut array)) => {
                array.push(Object::Reference(widget_id));
                acro.set("Fields", Object::Array(array));
            }
            _ => {
                acro.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
            }
        }
        if acro.get(b"DR").is_err() {
            let mut fonts = Dictionary::new();
            fonts.set("Helv", Object::Reference(font));
            let mut dr = Dictionary::new();
            dr.set("Font", Object::Dictionary(fonts));
            acro.set("DR", Object::Dictionary(dr));
        }
        if acro.get(b"DA").is_err() {
            acro.set("DA", Object::string_literal(b"/Helv 0 Tf 0 g".to_vec()));
        }
        acro.set("NeedAppearances", Object::Boolean(true));

        let acro_id = self.inner_mut().add_object(Object::Dictionary(acro));
        let catalog = self
            .inner_mut()
            .get_object_mut(root_id)
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        let dict = catalog
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        dict.set("AcroForm", Object::Reference(acro_id));
        Ok(())
    }

    /// Récupère le dictionnaire `/AcroForm` du catalogue.
    fn acroform_dict(&self) -> Option<&Dictionary> {
        let catalog_id = self
            .inner()
            .trailer
            .get(b"Root")
            .ok()?
            .as_reference()
            .ok()?;
        let catalog = self.inner().get_object(catalog_id).ok()?.as_dict().ok()?;
        let acro_ref = catalog.get(b"AcroForm").ok()?;
        match acro_ref {
            Object::Reference(id) => self.inner().get_object(*id).ok()?.as_dict().ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    /// Trouve le numéro de page (1-indexé) à partir d'une référence de page.
    fn page_number_of(&self, page_id: (u32, u16)) -> Option<usize> {
        for (num, id) in self.inner().get_pages() {
            if id == page_id {
                return Some(num as usize);
            }
        }
        None
    }
}

/// Propriétés modifiables d'un champ de formulaire existant.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FieldProperties {
    /// Nom complet du champ (préfixe de parent conservé).
    pub name: String,
    /// Infobulle (`/TU`).
    pub tooltip: String,
    /// Champ obligatoire (bit 2 de `/Ff`).
    pub required: bool,
    /// Champ en lecture seule (bit 1 de `/Ff`).
    pub read_only: bool,
    /// Champ texte multiligne (bit 13 de `/Ff`).
    pub multiline: bool,
    /// Options d'une liste, dans l'ordre. Ignoré pour les autres types.
    pub options: Vec<String>,
}

/// Format d'échange des valeurs de formulaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormDataFormat {
    /// Conteneur FDF (fichier de type PDF).
    Fdf,
    /// XFDF (XML).
    Xfdf,
    /// CSV (`name,value`).
    Csv,
    /// JSON.
    Json,
}

impl FormDataFormat {
    /// Nom lisible pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            FormDataFormat::Fdf => "FDF",
            FormDataFormat::Xfdf => "XFDF",
            FormDataFormat::Csv => "CSV",
            FormDataFormat::Json => "JSON",
        }
    }

    /// Extension de fichier, sans le point.
    pub fn extension(self) -> &'static str {
        match self {
            FormDataFormat::Fdf => "fdf",
            FormDataFormat::Xfdf => "xfdf",
            FormDataFormat::Csv => "csv",
            FormDataFormat::Json => "json",
        }
    }
}

impl Document {
    /// Applique `props` au champ dont le nom actuel est `field.name`.
    pub fn update_form_field(&mut self, field: &FormField, props: &FieldProperties) -> Result<()> {
        let object_id = self
            .read_form_fields()
            .into_iter()
            .find(|f| f.name == field.name)
            .map(|f| f.object_id)
            .ok_or_else(|| Error::InvalidInput(format!("champ « {} » introuvable", field.name)))?;

        // Le nom complet peut être hiérarchique : on n'écrit que le segment
        // local, et seulement quand le préfixe du parent est bien celui du nom
        // demandé. Un champ de premier niveau garde le nom saisi tel quel —
        // le tronquer au dernier point renommerait « a.b » en « b ».
        let local = match field.name.rsplit_once('.') {
            Some((prefix, _)) => props
                .name
                .strip_prefix(&format!("{prefix}."))
                .or_else(|| props.name.rsplit('.').next())
                .unwrap_or(&props.name),
            None => props.name.as_str(),
        };
        let is_choice = matches!(
            field.kind,
            FormFieldKind::Choice | FormFieldKind::MultiChoice
        );

        {
            let obj = self
                .inner_mut()
                .get_object_mut(object_id)
                .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
            let dict = obj
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;

            if !local.is_empty() {
                dict.set(
                    "T",
                    Object::string_literal(crate::edit::encode_pdf_literal(local)),
                );
            }
            dict.set(
                "TU",
                Object::string_literal(crate::edit::encode_pdf_literal(&props.tooltip)),
            );

            // Chaque drapeau est posé ou effacé sans toucher aux autres.
            let mut flags = dict
                .get(b"Ff")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            set_bit(&mut flags, 1, props.read_only);
            set_bit(&mut flags, 2, props.required);
            set_bit(&mut flags, 4096, props.multiline);
            dict.set("Ff", Object::Integer(flags));

            if is_choice && !props.options.is_empty() {
                let opts: Vec<Object> = props
                    .options
                    .iter()
                    .map(|o| Object::string_literal(crate::edit::encode_pdf_literal(o)))
                    .collect();
                dict.set("Opt", Object::Array(opts));
            }
        }

        self.touch();
        tracing::debug!(field = %field.name, "champ mis à jour");
        Ok(())
    }

    /// Sérialise chaque couple nom/valeur de champ.
    ///
    /// Le résultat est en octets : le FDF est un conteneur d'octets dont les
    /// littéraux sont en WinAnsi, une chaîne UTF-8 n'y suffit pas.
    pub fn export_form_data(&self, format: FormDataFormat) -> Result<Vec<u8>> {
        let pairs: Vec<(String, String)> = self
            .read_form_fields()
            .into_iter()
            .map(|f| (f.name, f.value.unwrap_or_default()))
            .collect();

        let out = match format {
            FormDataFormat::Csv => export_csv(&pairs).into_bytes(),
            FormDataFormat::Json => export_json(&pairs).into_bytes(),
            FormDataFormat::Xfdf => export_xfdf(&pairs).into_bytes(),
            FormDataFormat::Fdf => export_fdf(&pairs),
        };
        Ok(out)
    }

    /// Lit des couples nom/valeur et les écrit dans les champs. Renvoie le
    /// nombre de valeurs réellement appliquées.
    pub fn import_form_data(&mut self, format: FormDataFormat, data: &[u8]) -> Result<usize> {
        let pairs = match format {
            FormDataFormat::Csv => parse_csv(&String::from_utf8_lossy(data)),
            FormDataFormat::Json => parse_json(&String::from_utf8_lossy(data)),
            FormDataFormat::Xfdf => parse_xfdf(&String::from_utf8_lossy(data)),
            FormDataFormat::Fdf => parse_fdf(data),
        };

        let mut applied = 0usize;
        for (name, value) in pairs {
            let Some(field) = self
                .read_form_fields()
                .into_iter()
                .find(|f| f.name == name)
            else {
                // Nom inconnu : ignoré, ce n'est pas une erreur.
                continue;
            };
            self.set_field_value(&field, &value)?;
            applied += 1;
        }
        if applied > 0 {
            self.touch();
        }
        Ok(applied)
    }

    /// Écrit `/V` (et `/AS` pour les cases) sans contrôle de lecture seule.
    fn set_field_value(&mut self, field: &FormField, value: &str) -> Result<()> {
        let (v_obj, as_obj) = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => (
                Object::Name(value.as_bytes().to_vec()),
                Object::Name(value.as_bytes().to_vec()),
            ),
            _ => (
                Object::string_literal(crate::edit::encode_pdf_literal(value)),
                Object::Null,
            ),
        };

        let obj = self
            .inner_mut()
            .get_object_mut(field.object_id)
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        dict.set("V", v_obj);
        if !matches!(as_obj, Object::Null) {
            dict.set("AS", as_obj);
        }
        Ok(())
    }
}

/// Pose ou efface un bit d'un entier de drapeaux.
fn set_bit(flags: &mut i64, bit: i64, on: bool) {
    if on {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}

fn export_csv(pairs: &[(String, String)]) -> String {
    let mut out = String::from("name,value\n");
    for (name, value) in pairs {
        out.push_str(&csv_escape(name));
        out.push(',');
        out.push_str(&csv_escape(value));
        out.push('\n');
    }
    out
}

/// Échappe une chaîne pour un littéral JSON (sans `serde_json`).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn export_json(pairs: &[(String, String)]) -> String {
    let mut out = String::from("{\"fields\":[");
    for (index, (name, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&json_escape(name));
        out.push_str("\",\"value\":\"");
        out.push_str(&json_escape(value));
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

fn export_xfdf(pairs: &[(String, String)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\"><fields>",
    );
    for (name, value) in pairs {
        out.push_str("<field name=\"");
        out.push_str(&escape_xml(name));
        out.push_str("\"><value>");
        out.push_str(&escape_xml(value));
        out.push_str("</value></field>");
    }
    out.push_str("</fields></xfdf>");
    out
}

/// Encode une valeur en littéral PDF `(…)`, prêt à insérer.
///
/// Le résultat reste en octets : les littéraux FDF sont en WinAnsi, et passer
/// par une `String` remplaçait chaque octet ≥ 0x80 par U+FFFD.
fn pdf_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    out.push(b'(');
    out.extend_from_slice(&crate::edit::encode_pdf_literal(s));
    out.push(b')');
    out
}

fn export_fdf(pairs: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::from(&b"%FDF-1.2\n1 0 obj\n<< /FDF << /Fields [\n"[..]);
    for (name, value) in pairs {
        out.extend_from_slice(b"<< /T ");
        out.extend_from_slice(&pdf_literal(name));
        out.extend_from_slice(b" /V ");
        out.extend_from_slice(&pdf_literal(value));
        out.extend_from_slice(b" >>\n");
    }
    out.extend_from_slice(b"] >>\n>>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    out
}

/// Lit un littéral PDF `(…)` en partant de `start`, renvoie le texte décodé
/// et l'index suivant la parenthèse fermante.
fn read_pdf_literal(data: &[u8], start: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = start;
    while i < data.len() {
        let b = data[i];
        match b {
            b'\\' => {
                i += 1;
                if i >= data.len() {
                    break;
                }
                let e = data[i];
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0C),
                    b'0'..=b'7' => {
                        let mut val = (e - b'0') as u32;
                        let mut k = 0;
                        while k < 2 && i + 1 < data.len() && (b'0'..=b'7').contains(&data[i + 1]) {
                            i += 1;
                            val = val * 8 + (data[i] - b'0') as u32;
                            k += 1;
                        }
                        out.push(val as u8);
                    }
                    other => out.push(other),
                }
                i += 1;
            }
            b'(' => {
                // La parenthèse ouvrante du littéral n'appartient pas au
                // contenu ; celles imbriquées, si.
                if depth > 0 {
                    out.push(b'(');
                }
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
                out.push(b')');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    (crate::structure::decode_pdf_text(&out), i)
}

/// Lecteur tolérant de FDF : trouve les couples `/T` / `/V`.
fn parse_fdf(data: &[u8]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'/' && i + 1 < data.len() && (data[i + 1] == b'T' || data[i + 1] == b'V') {
            let key = data[i + 1];
            let mut j = i + 2;
            while j < data.len() && data[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < data.len() && data[j] == b'(' {
                let (text, next) = read_pdf_literal(data, j);
                if key == b'T' {
                    pending = Some(text);
                } else if let Some(name) = pending.take() {
                    pairs.push((name, text));
                }
                i = next;
                continue;
            }
        }
        i += 1;
    }
    pairs
}

fn parse_csv(data: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = data.chars().collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    field.push('"');
                    i += 2;
                } else {
                    in_quotes = false;
                    i += 1;
                }
            } else {
                field.push(c);
                i += 1;
            }
        } else {
            match c {
                '"' => {
                    in_quotes = true;
                    i += 1;
                }
                ',' => {
                    record.push(std::mem::take(&mut field));
                    i += 1;
                }
                '\r' => i += 1,
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut record));
                    i += 1;
                }
                _ => {
                    field.push(c);
                    i += 1;
                }
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        rows.push(record);
    }

    // La première ligne est l'en-tête `name,value`.
    rows.into_iter()
        .skip(1)
        .filter_map(|row| match row.len() {
            0 => None,
            1 => Some((row[0].clone(), String::new())),
            _ => Some((row[0].clone(), row[1].clone())),
        })
        .collect()
}

/// Lit une chaîne JSON en partant de `start` (le guillemet ouvrant).
fn read_json_string(chars: &[char], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                break;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                match chars[i] {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let mut code = 0u32;
                        let mut k = 0;
                        while k < 4 && i + 1 < chars.len() {
                            let Some(d) = chars[i + 1].to_digit(16) else {
                                break;
                            };
                            code = code * 16 + d;
                            i += 1;
                            k += 1;
                        }
                        if let Some(c) = char::from_u32(code) {
                            out.push(c);
                        }
                    }
                    other => out.push(other),
                }
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i)
}

/// Lecteur tolérant de JSON : associe les clés `name` et `value`.
fn parse_json(data: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = data.chars().collect();
    let mut pairs = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '"' {
            i += 1;
            continue;
        }
        let (key, after_key) = read_json_string(&chars, i);
        let mut j = after_key;
        while j < chars.len() && chars[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut consumed = after_key;
        if j < chars.len() && chars[j] == ':' {
            j += 1;
            while j < chars.len() && chars[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '"' {
                let (value, after_value) = read_json_string(&chars, j);
                match key.as_str() {
                    "name" => pending = Some(value),
                    "value" => {
                        if let Some(name) = pending.take() {
                            pairs.push((name, value));
                        }
                    }
                    _ => {}
                }
                consumed = after_value;
            }
        }
        i = consumed;
    }
    pairs
}

fn parse_xfdf(data: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut rest = data;
    while let Some(pos) = rest.find("name=\"") {
        rest = &rest[pos + 6..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let name = unescape_xml(&rest[..end]);
        rest = &rest[end + 1..];
        if let Some(vstart) = rest.find("<value>") {
            let after = &rest[vstart + 7..];
            if let Some(vend) = after.find("</value>") {
                pairs.push((name, unescape_xml(&after[..vend])));
                rest = &after[vend + 8..];
            }
        }
    }
    pairs
}
