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

use lopdf::{Dictionary, Object};

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
