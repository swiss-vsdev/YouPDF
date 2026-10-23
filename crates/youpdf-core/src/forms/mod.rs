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

mod read;
mod write;
mod import_export;

use lopdf::{Dictionary, Object};
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
/// Pose ou efface un bit d'un entier de drapeaux.
fn set_bit(flags: &mut i64, bit: i64, on: bool) {
    if on {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}
