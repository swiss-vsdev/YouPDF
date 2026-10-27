//! Page labels: `/PageLabels` name tree for custom page numbering.
//!
//! PDF spec: `/PageLabels` is a name tree in the catalog. Keys are
//! zero-based page indices as ASCII strings. Values are dictionaries:
//! - `/P` (prefix, optional string)
//! - `/S` (style, optional name: `/D`, `/r`, `/R`, `/a`, `/A`)
//! - `/St` (start value, optional integer, default 1)
//!
//! Styles:
//! - `/D` — Decimal arabic numerals (1, 2, 3…)
//! - `/r` — Lowercase roman (i, ii, iii…)
//! - `/R` — Uppercase roman (I, II, III…)
//! - `/a` — Lowercase letters (a, b, c…)
//! - `/A` — Uppercase letters (A, B, C…)

use std::collections::BTreeMap;

use lopdf::{dictionary, Dictionary, Object};

use crate::document::Document;
use youpdf_shared::{Error, Result};

/// Page label style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    /// Decimal arabic (1, 2, 3…)
    Arabic,
    /// Lowercase roman (i, ii, iii…)
    RomanLower,
    /// Uppercase roman (I, II, III…)
    RomanUpper,
    /// Lowercase letters (a, b, c…)
    LetterLower,
    /// Uppercase letters (A, B, C…)
    LetterUpper,
}

impl LabelStyle {
    /// PDF name for the style.
    pub fn pdf_name(&self) -> &'static [u8] {
        match self {
            LabelStyle::Arabic => b"D",
            LabelStyle::RomanLower => b"r",
            LabelStyle::RomanUpper => b"R",
            LabelStyle::LetterLower => b"a",
            LabelStyle::LetterUpper => b"A",
        }
    }

    /// Format a number in this style.
    pub fn format(&self, n: u32) -> String {
        match self {
            LabelStyle::Arabic => n.to_string(),
            LabelStyle::RomanLower => to_roman(n).to_lowercase(),
            LabelStyle::RomanUpper => to_roman(n),
            LabelStyle::LetterLower => to_letter(n).to_lowercase(),
            LabelStyle::LetterUpper => to_letter(n),
        }
    }
}

/// Page label definition.
#[derive(Debug, Clone)]
pub struct PageLabel {
    /// Prefix string (e.g., "A-", "App-").
    pub prefix: Option<String>,
    /// Numbering style.
    pub style: LabelStyle,
    /// Starting number (1-indexed).
    pub start: u32,
}

impl PageLabel {
    /// Create a new page label.
    pub fn new(prefix: Option<String>, style: LabelStyle, start: u32) -> Self {
        Self {
            prefix,
            style,
            start: start.max(1),
        }
    }

    /// Format the label for a given page index (0-based).
    pub fn format(&self, page_index: u32) -> String {
        let mut out = String::new();
        if let Some(prefix) = &self.prefix {
            out.push_str(prefix);
        }
        let num = self.start + page_index;
        out.push_str(&self.style.format(num));
        out
    }

    /// Convert to a PDF dictionary.
    pub fn to_dict(&self) -> Dictionary {
        let mut dict = Dictionary::new();
        if let Some(prefix) = &self.prefix {
            dict.set("P", Object::string_literal(prefix.as_bytes()));
        }
        dict.set("S", Object::Name(self.style.pdf_name().to_vec()));
        dict.set("St", Object::Integer(self.start as i64));
        dict
    }

    /// Parse from a PDF dictionary.
    pub fn from_dict(dict: &Dictionary) -> Result<Self> {
        let prefix = dict
            .get(b"P")
            .ok()
            .and_then(|o| Object::as_str(o).ok())
            .map(|s| String::from_utf8_lossy(s).to_string());
        let style = dict
            .get(b"S")
            .ok()
            .and_then(|o| Object::as_name(o).ok())
            .map_or(LabelStyle::Arabic, |name| match name {
                b"D" => LabelStyle::Arabic,
                b"r" => LabelStyle::RomanLower,
                b"R" => LabelStyle::RomanUpper,
                b"a" => LabelStyle::LetterLower,
                b"A" => LabelStyle::LetterUpper,
                _ => LabelStyle::Arabic,
            });
        let start = dict
            .get(b"St")
            .ok()
            .and_then(|o| Object::as_i64(o).ok())
            .map(|n| n as u32)
            .unwrap_or(1);
        Ok(Self::new(prefix, style, start))
    }
}

/// Convert a number to uppercase roman numerals.
pub fn to_roman(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, letters) in TABLE {
        while n >= value {
            out.push_str(letters);
            n -= value;
        }
    }
    out
}

/// Convert a number to uppercase letters (1=A, 26=Z, 27=AA…).
pub fn to_letter(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        let c = b'A' + (n % 26) as u8;
        out.push(c as char);
        n /= 26;
    }
    out.chars().rev().collect()
}

/// Get page labels from the document.
/// Returns a map of page_index -> label.
pub fn get_page_labels(doc: &Document) -> Result<BTreeMap<u32, PageLabel>> {
    let catalog = match doc.inner().catalog() {
        Ok(c) => c,
        Err(e) => return Err(Error::Pdf(format!("catalog: {e}"))),
    };

    let page_labels_id = match catalog.get(b"PageLabels") {
        Ok(obj) => match obj.as_reference() {
            Ok(id) => id,
            Err(_) => return Ok(BTreeMap::new()),
        },
        Err(_) => return Ok(BTreeMap::new()),
    };

    // Walk the name tree /Nums array.
    let labels_obj = match doc.inner().get_object(page_labels_id) {
        Ok(o) => o,
        Err(e) => return Err(Error::Pdf(format!("object: {e}"))),
    };
    let nums_array = match labels_obj.as_dict() {
        Ok(dict) => match dict.get(b"Nums") {
            Ok(obj) => match obj.as_array() {
                Ok(arr) => arr.clone(),
                Err(_) => return Err(Error::NotFound("PageLabels /Nums: not an array".into())),
            },
            Err(_) => return Err(Error::NotFound("PageLabels /Nums: missing".into())),
        },
        Err(_) => return Err(Error::NotFound("PageLabels: not a dictionary".into())),
    };

    let mut result = BTreeMap::new();
    let mut i = 0;
    while i + 1 < nums_array.len() {
        let key_obj = &nums_array[i];
        let val_obj = &nums_array[i + 1];
        i += 2;

        let page_index = key_obj
            .as_str()
            .ok()
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        if let Ok(dict) = val_obj.as_dict() {
            if let Ok(label) = PageLabel::from_dict(dict) {
                result.insert(page_index, label);
            }
        }
    }

    Ok(result)
}

/// Set page labels on the document.
/// `labels` is a map of starting page_index -> label definition.
/// The label applies to that page and all subsequent pages until
/// another label overrides it.
pub fn set_page_labels(doc: &mut Document, labels: &BTreeMap<u32, PageLabel>) -> Result<()> {
    if labels.is_empty() {
        // Remove PageLabels if empty.
        let catalog = doc.inner_mut().catalog_mut()
            .map_err(|e| Error::Pdf(format!("catalog: {e}")))?;
        catalog.remove(b"PageLabels");
        return Ok(());
    }

    // Build /Nums array: [key1, val1, key2, val2, ...]
    let mut nums = Vec::new();
    for (&page_index, label) in labels {
        let key = Object::string_literal(page_index.to_string().as_bytes());
        let val = Object::Dictionary(label.to_dict());
        nums.push(key);
        nums.push(val);
    }

    let nums_dict = dictionary! {
        b"Nums" => Object::Array(nums),
    };

    let labels_id = doc.inner_mut().add_object(Object::Dictionary(nums_dict));

    let catalog = doc.inner_mut().catalog_mut()
        .map_err(|e| Error::Pdf(format!("catalog: {e}")))?;
    catalog.set("PageLabels", Object::Reference(labels_id));

    Ok(())
}

/// Get the formatted label for a specific page (1-indexed page number).
pub fn get_page_label(doc: &Document, page_number: u32) -> Option<String> {
    let labels = get_page_labels(doc).ok()?;
    let page_index = page_number.saturating_sub(1);

    // Find the most recent label that applies to this page.
    let mut applicable: Option<(u32, &PageLabel)> = None;
    for (&idx, label) in &labels {
        if idx <= page_index {
            if applicable.is_none() || idx > applicable.unwrap().0 {
                applicable = Some((idx, label));
            }
        }
    }

    applicable.map(|(start_idx, label)| {
        label.format(page_index - start_idx)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roman() {
        assert_eq!(to_roman(1), "I");
        assert_eq!(to_roman(4), "IV");
        assert_eq!(to_roman(9), "IX");
        assert_eq!(to_roman(19), "XIX");
        assert_eq!(to_roman(49), "XLIX");
        assert_eq!(to_roman(99), "XCIX");
        assert_eq!(to_roman(499), "CDXCIX");
        assert_eq!(to_roman(999), "CMXCIX");
    }

    #[test]
    fn test_letter() {
        assert_eq!(to_letter(1), "A");
        assert_eq!(to_letter(26), "Z");
        assert_eq!(to_letter(27), "AA");
        assert_eq!(to_letter(52), "AZ");
        assert_eq!(to_letter(53), "BA");
    }

    #[test]
    fn test_label_format() {
        let label = PageLabel::new(Some("App-".to_string()), LabelStyle::RomanLower, 1);
        assert_eq!(label.format(0), "App-i");
        assert_eq!(label.format(1), "App-ii");
        assert_eq!(label.format(10), "App-xi");

        let label2 = PageLabel::new(None, LabelStyle::Arabic, 5);
        assert_eq!(label2.format(0), "5");
        assert_eq!(label2.format(10), "15");
    }
}
