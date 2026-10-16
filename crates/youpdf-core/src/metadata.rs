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

        let get_date = |key: &[u8]| -> Option<DateTime<Utc>> {
            get_str(key).and_then(|text| parse_pdf_date(&text))
        };

        Self {
            title: get_str(b"Title"),
            author: get_str(b"Author"),
            subject: get_str(b"Subject"),
            keywords: get_str(b"Keywords"),
            creator: get_str(b"Creator"),
            producer: get_str(b"Producer"),
            created_at: get_date(b"CreationDate"),
            modified_at: get_date(b"ModDate"),
            page_count: doc.get_pages().len(),
            file_size: None,
        }
    }
}

/// Lit une date PDF : `D:AAAAMMJJHHmmSS` suivie, si présente, d'un décalage
/// `+HH'mm'`, `-HH'mm'` ou `Z`.
///
/// Les producteurs réels tronquent volontiers la fin (`D:2024`) ou
/// l'écrivent sans le préfixe : on accepte ce qui est lisible et on rejette
/// le reste, plutôt que d'afficher une date inventée.
pub(crate) fn parse_pdf_date(text: &str) -> Option<DateTime<Utc>> {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix("D:").unwrap_or(trimmed);
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }

    let number = |range: std::ops::Range<usize>, default: u32| -> u32 {
        digits
            .get(range)
            .and_then(|part| part.parse().ok())
            .unwrap_or(default)
    };
    let (year, month, day) = (number(0..4, 1970), number(4..6, 1), number(6..8, 1));
    let (hour, minute, second) = (number(8..10, 0), number(10..12, 0), number(12..14, 0));

    let date = chrono::NaiveDate::from_ymd_opt(year as i32, month.clamp(1, 12), day)
        // Un jour impossible (31 février) ne doit pas faire perdre l'année.
        .or_else(|| chrono::NaiveDate::from_ymd_opt(year as i32, month.clamp(1, 12), 1))?;
    let time = chrono::NaiveTime::from_hms_opt(hour.min(23), minute.min(59), second.min(59))?;
    let naive = date.and_time(time);

    let rest = body.get(digits.len()..).unwrap_or("");
    let offset = if rest.starts_with('Z') {
        0
    } else if rest.starts_with('+') || rest.starts_with('-') {
        let sign = if rest.starts_with('-') { -1 } else { 1 };
        let hours: i64 = rest[1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let minutes: i64 = rest
            .split('\'')
            .nth(1)
            .and_then(|part| part.trim_end_matches('\'').parse().ok())
            .unwrap_or(0);
        sign * (hours * 3600 + minutes * 60)
    } else {
        0
    };

    Some(DateTime::from_naive_utc_and_offset(
        naive - chrono::Duration::seconds(offset),
        Utc,
    ))
}
