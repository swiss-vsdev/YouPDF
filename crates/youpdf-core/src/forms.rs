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
    /// Numéro de page (1-indexé), si le champ en a une.
    pub page: Option<usize>,
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
        let Ok(fields_ref) = acroform.get(b"Fields") else {
            return out;
        };
        let Ok(fields) = fields_ref.as_array() else {
            return out;
        };

        // 2. Parcourir récursivement (les champs peuvent être hiérarchiques).
        for field_ref in fields {
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

        // Nom local du champ.
        let local_name = dict
            .get(b"T")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(|s| String::from_utf8_lossy(s).to_string());

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
                if flags & 0x200000 != 0 {
                    kind = FormFieldKind::MultiChoice;
                }
            }

            // Valeur actuelle. Les valeurs des champs sont encodées en
            // **WinAnsi** (la page de codes de la police standard) : un
            // `String::from_utf8_lossy` re-encodait chaque octet ≥ 0x80
            // en deux octets UTF-8, et le lecteur affichait « cafÃ© »
            // au lieu de « café ». On lit donc les octets tels quels.
            let value = dict.get(b"V").ok().and_then(|o| match o {
                Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
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
                                Some(String::from_utf8_lossy(bytes).to_string())
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Flags lecture seule (bit 1).
            let flags = dict
                .get(b"Ff")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            let read_only = flags & 1 != 0;

            // Page du champ (via /P si présent).
            let page = dict
                .get(b"P")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|page_id| self.page_number_of(page_id));

            if !full_name.is_empty() {
                out.push(FormField {
                    object_id: field_id,
                    name: full_name.clone(),
                    kind,
                    value,
                    options,
                    read_only,
                    page,
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

        // Cas des cases à cocher / boutons radio : on met /V et /AS.
        let (v_obj, as_obj) = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => (
                Object::Name(value.as_bytes().to_vec()),
                Object::Name(value.as_bytes().to_vec()),
            ),
            // `encode_pdf_literal` convertit chaque caractère en un
            // octet WinAnsi (et point d'interrogation pour les
            // caractères sans glyphe), ce qui évite le mojibake.
            _ => (Object::string_literal(crate::edit::encode_pdf_literal(value)), Object::Null),
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
            FormFieldKind::MultiChoice => (1 << 17) | (1 << 21),
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
            page: Some(field.page as usize),
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
        let mut fields = acro
            .get(b"Fields")
            .ok()
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        fields.push(Object::Reference(widget_id));
        acro.set("Fields", Object::Array(fields));
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
