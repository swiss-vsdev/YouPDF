//! Extraction des métadonnées d'un document PDF.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Informations descriptives d'un document PDF.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// Date de création déclarée dans le PDF.
    pub created_at: Option<DateTime<Utc>>,
    /// Date de dernière modification déclarée dans le PDF.
    pub modified_at: Option<DateTime<Utc>>,
    /// Nombre de pages (recopié pour affichage direct).
    pub page_count: usize,
    /// Taille du fichier sur disque, si connue.
    pub file_size: Option<u64>,
}

impl DocumentMetadata {
    /// Construit les métadonnées à partir d'un document `lopdf`.
    pub(crate) fn from_lopdf(doc: &lopdf::Document) -> Self {
        use lopdf::Object;

        let info = doc
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|obj| obj.as_reference().ok())
            .and_then(|id| doc.get_object(id).ok())
            .and_then(|obj| obj.as_dict().ok());

        let get_str = |key: &[u8]| -> Option<String> {
            info.and_then(|d| d.get(key).ok())
                .and_then(|obj| match obj {
                    Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
                    _ => None,
                })
        };

        Self {
            title: get_str(b"Title"),
            author: get_str(b"Author"),
            subject: get_str(b"Subject"),
            keywords: get_str(b"Keywords"),
            creator: get_str(b"Creator"),
            producer: get_str(b"Producer"),
            created_at: None,
            modified_at: None,
            page_count: doc.get_pages().len(),
            file_size: None,
        }
    }
}
