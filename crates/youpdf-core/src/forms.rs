//! Formulaires AcroForm.
//!
//! ## Support
//!
//! - **Lecture** : liste des champs avec nom, type, valeur actuelle,
//!   options (pour les listes et boutons radio). `/FT`, `/Ff`, `/V`, `/Opt`
//!   et `/TU` sont hérités du parent quand le widget ne les porte pas, et un
//!   champ à plusieurs widgets (groupe radio) donne une entrée par widget
//!   avec son rectangle et sa page.
//! - **Remplissage** : modification de la valeur `/V` d'un champ,
//!   plus `/AS` (appearance state) sur chaque widget pour les cases et les
//!   boutons radio, et régénération du flux `/AP` pour le texte.
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
    /// Bouton poussoir (`/Btn` avec le bit 17).
    Button,
    /// Champ texte formaté en date (icône calendrier dans l'interface).
    Date,
    /// Champ code-barres (champ texte à police code-barres).
    Barcode,
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
            FormFieldKind::Button => "Bouton",
            FormFieldKind::Date => "Date",
            FormFieldKind::Barcode => "Code-barres",
            FormFieldKind::Other(_) => "Autre",
        }
    }
}

/// Alias de police standard pour `/DA`.
fn da_font_alias(font: &str) -> &'static str {
    match font {
        "Courier" => "Cour",
        "Times" => "TiRo",
        _ => "Helv",
    }
}

/// Action JavaScript (`/S /JavaScript`, `/JS`).
fn js_action(script: &str) -> Object {
    let mut dict = Dictionary::new();
    dict.set("S", Object::Name(b"JavaScript".to_vec()));
    dict.set(
        "JS",
        Object::string_literal(crate::edit::encode_pdf_literal(script)),
    );
    Object::Dictionary(dict)
}

/// Un nombre PDF, entier ou réel, en `f32`.
fn number_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => object.as_i64().ok().map(|value| value as f32),
    }
}

/// Attributs qu'un champ transmet à ses sous-champs.
#[derive(Debug, Clone, Default)]
struct Inherited {
    ft: Option<Vec<u8>>,
    flags: i64,
    value: Option<String>,
    options: Option<Vec<String>>,
    tooltip: Option<String>,
    page: Option<usize>,
    rect: Option<[f32; 4]>,
}

/// Valeur d'un `/V` ou `/DV` : chaîne ou nom.
fn field_value(object: Option<&Object>) -> Option<String> {
    match object? {
        Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(bytes)),
        Object::Name(name) => Some(crate::structure::decode_pdf_text(name)),
        _ => None,
    }
}

/// Options d'un `/Opt`. Une entrée peut être une chaîne, ou un tableau
/// `[valeur exportée, libellé]`.
fn field_options(object: Option<&Object>) -> Option<Vec<String>> {
    let items = object?.as_array().ok()?;
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(bytes)),
                Object::Array(pair) => pair.last().and_then(|value| field_value(Some(value))),
                Object::Name(name) => Some(crate::structure::decode_pdf_text(name)),
                _ => None,
            })
            .collect(),
    )
}

/// Rectangle d'un dictionnaire, s'il est complet.
fn field_rect(dict: &Dictionary) -> Option<[f32; 4]> {
    let values: Vec<f32> = dict
        .get(b"Rect")
        .ok()?
        .as_array()
        .ok()?
        .iter()
        .filter_map(number_f32)
        .collect();
    (values.len() >= 4).then(|| {
        [
            values[0].min(values[2]),
            values[1].min(values[3]),
            values[0].max(values[2]),
            values[1].max(values[3]),
        ]
    })
}

/// États d'apparence d'un widget (`/AP /N`), hors `Off`.
fn appearance_states(dict: &Dictionary) -> Vec<String> {
    let Ok(states) = dict
        .get(b"AP")
        .and_then(|o| o.as_dict())
        .and_then(|ap| ap.get(b"N"))
        .and_then(|o| o.as_dict())
    else {
        return Vec::new();
    };
    states
        .iter()
        .filter_map(|(key, _)| {
            let name = crate::structure::decode_pdf_text(key);
            (name != "Off").then_some(name)
        })
        .collect()
}

/// Un champ de formulaire lu depuis un PDF.
#[derive(Debug, Clone)]
pub struct FormField {
    /// Référence du dictionnaire de champ (pour `/V`, `/T`, les propriétés).
    pub object_id: (u32, u16),
    /// Objet du widget qui porte ce rectangle et cet état.
    pub widget_id: (u32, u16),
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
    /// Rectangle du widget en repère page : `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// États d'apparence du widget (cases à cocher et boutons radio).
    pub states: Vec<String>,
    /// État courant du widget (`/AS`).
    pub state: Option<String>,
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
    ///
    /// Un champ peut hériter de `/FT`, `/Ff`, `/V`, `/Opt` et `/TU` de son
    /// parent, et ses widgets peuvent être des objets séparés sans `/T` :
    /// chaque widget devient une entrée avec son propre rectangle et sa page.
    pub fn read_form_fields(&self) -> Vec<FormField> {
        let mut out = Vec::new();

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

        for field_ref in &fields {
            let Ok(field_id) = field_ref.as_reference() else {
                continue;
            };
            self.walk_field(field_id, None, &Inherited::default(), &mut out);
        }

        out
    }

    /// Parcourt un champ, ses sous-champs et ses widgets.
    fn walk_field(
        &self,
        field_id: (u32, u16),
        parent_name: Option<&str>,
        inherited: &Inherited,
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

        // Attributs hérités : le dictionnaire le plus proche gagne.
        let ft = dict
            .get(b"FT")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(<[u8]>::to_vec)
            .or_else(|| inherited.ft.clone());
        let flags = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(inherited.flags);
        let value = field_value(dict.get(b"V").ok()).or_else(|| inherited.value.clone());
        let options = field_options(dict.get(b"Opt").ok())
            .or_else(|| inherited.options.clone())
            .unwrap_or_default();
        let tooltip = dict
            .get(b"TU")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(crate::structure::decode_pdf_text)
            .or_else(|| inherited.tooltip.clone())
            .unwrap_or_default();
        let page = dict
            .get(b"P")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .and_then(|page_id| self.page_number_of(page_id).map(|num| num as usize))
            .or(inherited.page);
        let rect = field_rect(dict).or(inherited.rect);

        // Enfants : les vrais sous-champs portent un `/T` (ou ont leurs
        // propres enfants) ; les autres sont des widgets.
        let kids: Vec<(u32, u16)> = dict
            .get(b"Kids")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|items| items.iter().filter_map(|item| item.as_reference().ok()).collect())
            .unwrap_or_default();
        let mut child_fields = Vec::new();
        let mut widgets = Vec::new();
        for kid in kids {
            let Ok(kid_obj) = self.inner().get_object(kid) else {
                continue;
            };
            let Ok(kid_dict) = kid_obj.as_dict() else {
                continue;
            };
            if kid_dict.get(b"T").is_ok() || kid_dict.get(b"Kids").is_ok() {
                child_fields.push(kid);
            } else {
                widgets.push(kid);
            }
        }

        if !child_fields.is_empty() {
            let next = Inherited {
                ft,
                flags,
                value,
                options: Some(options),
                tooltip: Some(tooltip),
                page,
                rect,
            };
            for kid in child_fields {
                self.walk_field(kid, Some(&full_name), &next, out);
            }
            return;
        }

        let Some(ft) = ft else {
            return;
        };
        if full_name.is_empty() {
            return;
        }
        let mut kind = FormFieldKind::from_ft(&ft);
        if matches!(kind, FormFieldKind::Checkbox) {
            // Bit 16 : bouton radio ; bit 17 : bouton poussoir.
            if flags & 0x8000 != 0 {
                kind = FormFieldKind::Radio;
            } else if flags & 0x10000 != 0 {
                kind = FormFieldKind::Button;
            }
        }
        if matches!(kind, FormFieldKind::Choice) && flags & (1 << 20) != 0 {
            kind = FormFieldKind::MultiChoice;
        }

        let widgets = if widgets.is_empty() {
            vec![field_id]
        } else {
            widgets
        };
        for widget_id in widgets {
            let widget = self
                .inner()
                .get_object(widget_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .unwrap_or(dict);
            let rect = field_rect(widget).or(rect).unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let page = widget
                .get(b"P")
                .ok()
                .and_then(|o| o.as_reference().ok())
                .and_then(|page_id| self.page_number_of(page_id).map(|num| num as usize))
                .or_else(|| self.page_of_annotation(widget_id))
                .or(page);
            out.push(FormField {
                object_id: field_id,
                widget_id,
                name: full_name.clone(),
                kind: kind.clone(),
                value: value.clone(),
                options: options.clone(),
                read_only: flags & 1 != 0,
                tooltip: tooltip.clone(),
                required: flags & 2 != 0,
                multiline: flags & 4096 != 0,
                page,
                rect,
                states: appearance_states(widget),
                state: widget
                    .get(b"AS")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .map(crate::structure::decode_pdf_text),
            });
        }
    }

    /// Page qui référence cette annotation dans son `/Annots`.
    fn page_of_annotation(&self, annot_id: (u32, u16)) -> Option<usize> {
        for (number, page_id) in self.inner().get_pages() {
            let Ok(page) = self.inner().get_object(page_id).and_then(|obj| obj.as_dict()) else {
                continue;
            };
            let Ok(annots) = page.get(b"Annots").and_then(|o| o.as_array()) else {
                continue;
            };
            if annots
                .iter()
                .any(|item| item.as_reference().ok() == Some(annot_id))
            {
                return Some(number as usize);
            }
        }
        None
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
            // Bit 17 : bouton poussoir (pas d'état coché).
            FormFieldKind::Button => 1 << 16,
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
            // Un bouton poussoir reste un `/Btn` : le drapeau 17 suffit.
            FormFieldKind::Button => b"Btn",
            // Un code-barres PDF est un champ texte à police dédiée.
            FormFieldKind::Date | FormFieldKind::Barcode => b"Tx",
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

        let states = match field.kind {
            FormFieldKind::Checkbox | FormFieldKind::Radio => vec!["Yes".to_string()],
            _ => Vec::new(),
        };
        Ok(FormField {
            object_id: widget_id,
            widget_id,
            name: field.name.clone(),
            kind: field.kind.clone(),
            value: field.value.clone(),
            options: field.options.clone(),
            read_only: field.read_only,
            tooltip: String::new(),
            required: false,
            multiline: false,
            page: Some(field.page as usize),
            rect,
            states,
            state: field
                .value
                .clone()
                .filter(|_| is_choice_state),
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

}

/// Propriétés modifiables d'un champ de formulaire existant.
#[derive(Debug, Clone, PartialEq)]
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
    /// Liste déroulante éditable (bit 19 de `/Ff`).
    pub editable: bool,
    /// Visibilité : 0 visible, 1 masqué, 2 visible non imprimable,
    /// 3 masqué mais imprimable.
    pub visibility: u8,
    /// Valeur par défaut (`/DV`), appliquée par « Reset form ».
    pub default_value: String,
    /// Saisie masquée (bit 14 de `/Ff`).
    pub password: bool,
    /// Peigne de N caractères (bit 25 de `/Ff`), 0 = désactivé.
    pub comb: u8,
    /// Alignement du texte (`/Q`) : 0 gauche, 1 centre, 2 droite.
    pub alignment: u8,
    /// Épaisseur de bordure (`/BS /W`).
    pub border_width: f32,
    /// Style de bordure (`/BS /S`) : 0 plein, 1 tireté, 2 biseau, 3 creux,
    /// 4 souligné.
    pub border_style: u8,
    /// Couleur de bordure (`/MK /BC`).
    pub border_color: [u8; 3],
    /// Couleur de fond (`/MK /BG`), appliquée quand `fill` est vrai.
    pub fill_color: [u8; 3],
    /// Remplir le fond.
    pub fill: bool,
    /// Police d'apparence (`/DA`).
    pub font: String,
    /// Corps de la police (`/DA`), 0 = automatique.
    pub font_size: f32,
    /// Couleur du texte (`/DA`).
    pub text_color: [u8; 3],
    /// Action JavaScript sur clic (`/AA /E`).
    pub action_js: String,
    /// Script de format (`/AA /F`).
    pub format_js: String,
    /// Script de calcul (`/AA /C`).
    pub calc_js: String,
}

impl Default for FieldProperties {
    fn default() -> Self {
        Self {
            name: String::new(),
            tooltip: String::new(),
            required: false,
            read_only: false,
            multiline: false,
            options: Vec::new(),
            editable: false,
            visibility: 0,
            default_value: String::new(),
            password: false,
            comb: 0,
            alignment: 0,
            border_width: 1.0,
            border_style: 0,
            border_color: [0, 0, 0],
            fill_color: [255, 255, 255],
            fill: false,
            font: "Helvetica".to_string(),
            font_size: 0.0,
            text_color: [0, 0, 0],
            action_js: String::new(),
            format_js: String::new(),
            calc_js: String::new(),
        }
    }
}

impl FieldProperties {
    /// Propriétés d'un champ existant, avec ses réglages courants.
    pub fn from_field(field: &FormField) -> Self {
        Self {
            name: field.name.clone(),
            tooltip: field.tooltip.clone(),
            required: field.required,
            read_only: field.read_only,
            multiline: field.multiline,
            options: field.options.clone(),
            visibility: 0,
            ..Self::default()
        }
    }
}
/// Lit les réglages d'apparence, de visibilité et d'action d'un champ.
impl Document {
    /// Relit les propriétés éditables d'un champ depuis son dictionnaire.
    pub fn read_field_properties(&self, field: &FormField) -> FieldProperties {
        let mut props = FieldProperties::from_field(field);
        let Ok(obj) = self.inner().get_object(field.object_id) else {
            return props;
        };
        let Ok(dict) = obj.as_dict() else {
            return props;
        };

        let flags = dict
            .get(b"Ff")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        props.password = flags & (1 << 13) != 0;
        props.comb = if flags & (1 << 24) != 0 {
            dict.get(b"MaxLen")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0)
                .clamp(0, 20) as u8
        } else {
            0
        };
        props.editable = flags & (1 << 18) != 0;
        props.alignment = dict
            .get(b"Q")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0)
            .clamp(0, 2) as u8;

        let field_flags = dict
            .get(b"F")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        props.visibility = if field_flags & 1 != 0 {
            if field_flags & 2 != 0 {
                3
            } else {
                1
            }
        } else if field_flags & 32 != 0 {
            2
        } else {
            0
        };

        props.default_value = dict
            .get(b"DV")
            .ok()
            .and_then(|o| match o {
                Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(bytes)),
                Object::Name(name) => Some(crate::structure::decode_pdf_text(name)),
                _ => None,
            })
            .unwrap_or_default();

        if let Ok(mk) = dict.get(b"MK").and_then(|o| o.as_dict()) {
            if let Ok(bc) = mk.get(b"BC").and_then(|o| o.as_array()) {
                let channels: Vec<u8> = bc
                    .iter()
                    .filter_map(number_f32)
                    .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
                    .collect();
                if channels.len() >= 3 {
                    props.border_color = [channels[0], channels[1], channels[2]];
                }
            }
            if let Ok(bg) = mk.get(b"BG").and_then(|o| o.as_array()) {
                let channels: Vec<u8> = bg
                    .iter()
                    .filter_map(number_f32)
                    .map(|value| (value * 255.0).round().clamp(0.0, 255.0) as u8)
                    .collect();
                if channels.len() >= 3 {
                    props.fill_color = [channels[0], channels[1], channels[2]];
                    props.fill = true;
                }
            }
        }

        if let Ok(bs) = dict.get(b"BS").and_then(|o| o.as_dict()) {
            props.border_width = bs
                .get(b"W")
                .ok()
                .and_then(number_f32)
                .unwrap_or(1.0);
            props.border_style = match bs.get(b"S").ok().and_then(|o| o.as_name().ok()) {
                Some(b"D") => 1,
                Some(b"B") => 2,
                Some(b"I") => 3,
                Some(b"U") => 4,
                _ => 0,
            };
        }

        if let Ok(da) = dict.get(b"DA").and_then(|o| o.as_str()) {
            let text = String::from_utf8_lossy(da);
            let parts: Vec<&str> = text.split_whitespace().collect();
            if let Some(size) = parts
                .iter()
                .position(|part| *part == "Tf")
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| parts.get(index))
                .and_then(|value| value.parse::<f32>().ok())
            {
                props.font_size = size;
            }
            if let Some(index) = parts.iter().position(|part| *part == "rg") {
                let channels: Vec<f32> = parts[index.saturating_sub(3)..index]
                    .iter()
                    .filter_map(|value| value.parse::<f32>().ok())
                    .collect();
                if channels.len() == 3 {
                    props.text_color = [
                        (channels[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (channels[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (channels[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    ];
                }
            }
        }

        if let Ok(aa) = dict.get(b"AA").and_then(|o| o.as_dict()) {
            let script = |key: &[u8]| -> String {
                aa.get(key)
                    .ok()
                    .and_then(|action| action.as_dict().ok())
                    .and_then(|action| action.get(b"JS").ok())
                    .and_then(|js| js.as_str().ok())
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                    .unwrap_or_default()
            };
            props.action_js = script(b"E");
            props.format_js = script(b"F");
            props.calc_js = script(b"C");
        }

        props
    }
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
    /// Remet tous les champs à leur valeur par défaut (`/DV`), ou les vide.
    ///
    /// Renvoie le nombre de champs touchés.
    pub fn reset_form_fields(&mut self) -> Result<usize> {
        let fields = self.read_form_fields();
        let mut count = 0;
        for field in fields {
            if field.read_only {
                continue;
            }
            let default = self
                .inner()
                .get_object(field.object_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .and_then(|dict| dict.get(b"DV").ok().cloned())
                .and_then(|obj| match obj {
                    Object::String(bytes, _) => Some(crate::structure::decode_pdf_text(&bytes)),
                    Object::Name(name) => Some(crate::structure::decode_pdf_text(&name)),
                    _ => None,
                });
            let value = match field.kind {
                FormFieldKind::Checkbox | FormFieldKind::Radio | FormFieldKind::Button => {
                    default.unwrap_or_else(|| "Off".to_string())
                }
                _ => default.unwrap_or_default(),
            };
            if self.fill_form_field(&field, &value).is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Déplace et redimensionne un champ de formulaire.
    ///
    /// Met à jour le `/Rect` du widget et, quand le champ porte ses propres
    /// limites, celui du dictionnaire de champ. L'apparence est régénérée par
    /// les lecteurs (`/NeedAppearances` reste vrai).
    pub fn set_form_field_rect(&mut self, field: &FormField, rect: [f32; 4]) -> Result<()> {
        if rect[2] - rect[0] < 2.0 || rect[3] - rect[1] < 2.0 {
            return Err(Error::InvalidInput(
                "un champ doit mesurer au moins 2 × 2 points".to_string(),
            ));
        }
        let array = vec![
            Object::Real(rect[0]),
            Object::Real(rect[1]),
            Object::Real(rect[2]),
            Object::Real(rect[3]),
        ];
        let obj = self
            .inner_mut()
            .get_object_mut(field.widget_id)
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ « {} » : {e}", field.name)))?;
        dict.set("Rect", Object::Array(array));
        Ok(())
    }

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

            if is_choice && !props.options.is_empty() {
                let opts: Vec<Object> = props
                    .options
                    .iter()
                    .map(|o| Object::string_literal(crate::edit::encode_pdf_literal(o)))
                    .collect();
                dict.set("Opt", Object::Array(opts));
                // Bit 19 : l'utilisateur peut saisir une valeur absente de la
                // liste (liste déroulante éditable).
                set_bit(&mut flags, 1 << 18, props.editable);
            }
            // Bits 14 (mot de passe) et 25 (peigne).
            set_bit(&mut flags, 1 << 13, props.password);
            set_bit(&mut flags, 1 << 24, props.comb > 0);
            if props.comb > 0 {
                dict.set("MaxLen", Object::Integer(props.comb as i64));
            }
            dict.set("Ff", Object::Integer(flags));

            // Alignement du texte.
            dict.set("Q", Object::Integer(props.alignment.min(2) as i64));

            // Visibilité : bit 2 masqué, bit 3 imprimable, bit 6 masqué à
            // l'écran.
            let mut field_flags = dict
                .get(b"F")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(0);
            set_bit(&mut field_flags, 1, props.visibility == 1 || props.visibility == 3);
            set_bit(&mut field_flags, 2, props.visibility != 2);
            set_bit(&mut field_flags, 32, props.visibility == 2);
            dict.set("F", Object::Integer(field_flags));

            // Valeur par défaut.
            if !props.default_value.is_empty() {
                dict.set(
                    "DV",
                    Object::string_literal(crate::edit::encode_pdf_literal(&props.default_value)),
                );
            }

            // Apparence : bordure et fond.
            let mut mk = Dictionary::new();
            mk.set(
                "BC",
                Object::Array(
                    props
                        .border_color
                        .iter()
                        .map(|c| Object::Real(*c as f32 / 255.0))
                        .collect(),
                ),
            );
            if props.fill {
                mk.set(
                    "BG",
                    Object::Array(
                        props
                            .fill_color
                            .iter()
                            .map(|c| Object::Real(*c as f32 / 255.0))
                            .collect(),
                    ),
                );
            }
            dict.set("MK", Object::Dictionary(mk));

            let style = match props.border_style {
                1 => "D",
                2 => "B",
                3 => "I",
                4 => "U",
                _ => "S",
            };
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(props.border_width.clamp(0.0, 12.0)));
            bs.set("S", Object::Name(style.as_bytes().to_vec()));
            dict.set("BS", Object::Dictionary(bs));

            // `/DA` : police, corps et couleur du texte.
            let size = if props.font_size > 0.0 {
                props.font_size
            } else {
                12.0
            };
            let [r, g, b] = props.text_color.map(|c| c as f32 / 255.0);
            let da = format!(
                "/{} {} Tf {r:.3} {g:.3} {b:.3} rg",
                da_font_alias(&props.font),
                size
            );
            dict.set("DA", Object::string_literal(da.as_bytes().to_vec()));

            // Actions, format et calcul.
            let mut aa = Dictionary::new();
            if !props.action_js.is_empty() {
                aa.set("E", js_action(&props.action_js));
            }
            if !props.format_js.is_empty() {
                aa.set("F", js_action(&props.format_js));
            }
            if !props.calc_js.is_empty() {
                aa.set("C", js_action(&props.calc_js));
            }
            if !aa.is_empty() {
                dict.set("AA", Object::Dictionary(aa));
            }
        }

        // Les apparences ne sont plus à jour : les lecteurs les régénèrent.
        if let Some(acro) = self.acroform_dict().cloned() {
            let mut acro = acro;
            acro.set("NeedAppearances", Object::Boolean(true));
            if let Ok(root) = self
                .inner()
                .trailer
                .get(b"Root")
                .and_then(|o| o.as_reference())
            {
                if let Ok(catalog) = self.inner_mut().get_object_mut(root) {
                    if let Ok(catalog) = catalog.as_dict_mut() {
                        catalog.set("AcroForm", Object::Dictionary(acro));
                    }
                }
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
        // Un groupe radio apparaît une fois par widget : l'export ne garde
        // qu'une ligne par champ.
        let mut seen = std::collections::HashSet::new();
        let pairs: Vec<(String, String)> = self
            .read_form_fields()
            .into_iter()
            .filter(|field| seen.insert(field.object_id))
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
        let button = matches!(
            field.kind,
            FormFieldKind::Checkbox | FormFieldKind::Radio | FormFieldKind::Button
        );
        if button {
            let selected = if value.is_empty() || value == "Off" {
                "Off".to_string()
            } else {
                self.resolve_button_state(field, value)
            };
            self.set_dict_name(field.object_id, "V", &selected)?;
            for widget_id in self.field_widget_ids(field) {
                let state = if selected == "Off" {
                    "Off".to_string()
                } else if widget_id == field.widget_id {
                    selected.clone()
                } else {
                    let states = self.widget_states(widget_id);
                    if states.iter().any(|state| state == &selected) {
                        selected.clone()
                    } else {
                        "Off".to_string()
                    }
                };
                self.set_dict_name(widget_id, "AS", &state)?;
            }
            return Ok(());
        }

        self.set_dict_string(field.object_id, "V", value)?;
        // Le texte doit rester lisible dans un lecteur qui ne régénère pas
        // les apparences : on écrit un nouveau flux `/AP`.
        let font = self.ensure_appearance_font()?;
        for widget_id in self.field_widget_ids(field) {
            let (w, h) = self.widget_size(widget_id, field.rect);
            let ap = self.add_appearance(Object::Reference(font), w, h, Some(value), false)?;
            self.set_widget_appearance(widget_id, ap)?;
        }
        Ok(())
    }

    /// Choisit l'état d'apparence réel d'un bouton.
    ///
    /// Les producteurs nomment rarement l'état `Yes` : `/On`, `/1`, `/Choice2`
    /// sont courants. Une valeur inconnue retombe sur le premier état du
    /// widget plutôt que d'écrire un `/AS` qui n'existe pas.
    fn resolve_button_state(&self, field: &FormField, value: &str) -> String {
        for widget_id in self.field_widget_ids(field) {
            let states = self.widget_states(widget_id);
            if let Some(state) = states.iter().find(|state| state.as_str() == value) {
                return state.clone();
            }
        }
        if let Some(state) = field
            .states
            .iter()
            .find(|state| state.as_str() == value)
        {
            return state.clone();
        }
        for widget_id in self.field_widget_ids(field) {
            if let Some(state) = self.widget_states(widget_id).into_iter().next() {
                return state;
            }
        }
        if let Some(state) = field.states.first() {
            return state.clone();
        }
        value.to_string()
    }

    /// Widgets d'un champ : le sien, plus ses enfants qui n'ont pas de `/T`.
    fn field_widget_ids(&self, field: &FormField) -> Vec<(u32, u16)> {
        let mut ids = vec![field.widget_id];
        if let Ok(dict) = self
            .inner()
            .get_object(field.object_id)
            .and_then(|obj| obj.as_dict())
        {
            if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
                for kid in kids {
                    let Ok(id) = kid.as_reference() else {
                        continue;
                    };
                    let Ok(kid_dict) = self.inner().get_object(id).and_then(|obj| obj.as_dict())
                    else {
                        continue;
                    };
                    if kid_dict.get(b"T").is_err() && !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    /// États d'apparence d'un widget, hors `Off`.
    fn widget_states(&self, widget_id: (u32, u16)) -> Vec<String> {
        self.inner()
            .get_object(widget_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .map(appearance_states)
            .unwrap_or_default()
    }

    fn widget_size(&self, widget_id: (u32, u16), fallback: [f32; 4]) -> (f32, f32) {
        let rect = self
            .inner()
            .get_object(widget_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .and_then(field_rect)
            .unwrap_or(fallback);
        (
            (rect[2] - rect[0]).max(4.0),
            (rect[3] - rect[1]).max(4.0),
        )
    }

    fn set_dict_name(&mut self, object_id: (u32, u16), key: &str, value: &str) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(object_id)
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        dict.set(key, Object::Name(value.as_bytes().to_vec()));
        Ok(())
    }

    fn set_dict_string(&mut self, object_id: (u32, u16), key: &str, value: &str) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(object_id)
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("champ : {e}")))?;
        dict.set(
            key,
            Object::string_literal(crate::edit::encode_pdf_literal(value)),
        );
        Ok(())
    }

    fn set_widget_appearance(&mut self, widget_id: (u32, u16), stream: ObjectId) -> Result<()> {
        let obj = self
            .inner_mut()
            .get_object_mut(widget_id)
            .map_err(|e| Error::Pdf(format!("widget : {e}")))?;
        let dict = obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("widget : {e}")))?;
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(stream));
        dict.set("AP", Object::Dictionary(ap));
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
