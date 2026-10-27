//! Jeux de codes (regex codifié) pour détecter les informations sensibles
//! dans le texte d'une page PDF.
//!
//! Utilise le crate `regex` — déjà disponible comme dépendance transitive.
use regex::Regex;

/// Une entrée du jeu de codes : un nom, une regex compilée et un exemple.
#[derive(Debug, Clone)]
pub struct CodeSetEntry {
    pub name: &'static str,
    pub pattern: &'static str,
    pub example: &'static str,
}

/// Jeux de codes par défaut (SSN, cartes de crédit, email, téléphone, dates,
/// Privacy Act, FOIA).
pub const DEFAULT_CODE_SETS: &[CodeSetEntry] = &[
    CodeSetEntry {
        name: "U.S. Social Security Number",
        pattern: r"\b\d{3}-\d{2}-\d{4}\b",
        example: "123-45-6789",
    },
    CodeSetEntry {
        name: "Credit Card (Visa/MC/AmEx)",
        pattern: r"\b4\d{3}[ -]?\d{4}[ -]?\d{4}[ -]?\d{1,4}\b|\b5[1-5]\d{2}[ -]?\d{4}[ -]?\d{4}[ -]?\d{4}\b|\b3[47]\d[ -]?\d{6}[ -]?\d{5}\b",
        example: "4111-1111-1111-1111",
    },
    CodeSetEntry {
        name: "Email Address",
        pattern: r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b",
        example: "test@example.com",
    },
    CodeSetEntry {
        name: "Phone Number (US)",
        pattern: r"\b(?:\+1[-.]?)?\(?\d{3}\)?[ .-]?\d{3}[ .-]?\d{4}\b",
        example: "(555) 123-4567",
    },
    CodeSetEntry {
        name: "Date (MM/DD/YYYY)",
        pattern: r"\b\d{1,2}/\d{1,2}/\d{4}\b",
        example: "12/31/2023",
    },
    CodeSetEntry {
        name: "U.S. Privacy Act Exemption",
        pattern: r"\b5 U\.S\.C\. § 552a\b",
        example: "5 U.S.C. § 552a",
    },
    CodeSetEntry {
        name: "FOIA Exemption",
        pattern: r"\b5 U\.S\.C\. § 552\(b\)\(\d\)",
        example: "5 U.S.C. § 552(b)(6)",
    },
    CodeSetEntry {
        name: "IBAN",
        pattern: r"\b[A-Z]{2}[0-9]{2}[A-Z0-9]{11,30}\b",
        example: "GB82WEST12345698765432",
    },
];

/// Compile every pattern in `code_sets` into a `Regex` and find all matches
/// in `text`. Returns `Vec<(start, end, name)>`.
pub fn find_code_matches(text: &str, code_sets: &[CodeSetEntry]) -> Vec<(usize, usize, &'static str)> {
    let mut compiled: Vec<(Regex, &'static str)> = Vec::with_capacity(code_sets.len());
    for entry in code_sets {
        // Safety: patterns are &'static str literals that are valid regexes
        let re = Regex::new(entry.pattern).expect("invalid code-set regex pattern");
        compiled.push((re, entry.name));
    }

    let mut matches = Vec::new();
    for (re, name) in &compiled {
        for m in re.find_iter(text) {
            matches.push((m.start(), m.end(), *name));
        }
    }
    // Sort by start position, then by length (shorter first at same start)
    matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    matches
}

/// Mark all code-set matches on a page as RedactionMarks.
/// (Simplified version — converts text positions to approximate bounding boxes.)
pub fn mark_code_matches(doc: &crate::document::Document, page: usize, code_sets: &[CodeSetEntry]) -> youpdf_shared::Result<Vec<crate::redact::RedactionMark>> {
    use crate::redact::RedactionMark;
    use crate::redact::RedactionFill;
    use crate::search::SearchIndex;

    let index = SearchIndex::build(doc)?;
    let text = index.text();
    let matches = find_code_matches(text, code_sets);

    // Approximate bounding boxes: each match gets a rect spanning the
    // full page width at a height proportional to its position.
    // Precise per-character geometry requires text fragment access
    // that is not exposed publicly — the user fine-tunes in the UI.
    let page_w = 612.0f32; // default letter width
    let page_h = 792.0f32; // default letter height
    let total = text.len().max(1);

    let mut marks = Vec::new();
    for (start, end, _name) in &matches {
        let y_frac = (*start as f32) / (total as f32);
        let h_frac = ((*end - *start) as f32) / (total as f32).max(0.001);
        let y = page_h * (1.0 - y_frac - h_frac.max(0.01));
        let h = page_h * h_frac.max(0.01);
        marks.push(RedactionMark {
            page,
            rect: [0.0, y, page_w, y + h],
            fill: RedactionFill::Black,
            overlay: None,
        });
    }
    Ok(marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssn_detection() {
        let matches = find_code_matches("SSN: 123-45-6789", DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "U.S. Social Security Number");
    }

    #[test]
    fn ssn_no_false_positive() {
        // "123-45-6789" seul sans contexte mot doit quand même matcher
        // car les word‑boundaries protègent contre le milieu d'un plus grand mot.
        let matches =
            find_code_matches("abc123-45-6789xyz", DEFAULT_CODE_SETS);
        // Le  avant le 1 empêche la correspondance si il y a des word chars avant.
        // Ici '3' est précédé par 'c' (word char) → pas de boundary → pas de match.
        assert!(
            matches.is_empty(),
            "SSN should not match inside 'abc123-45-6789xyz', got {matches:?}"
        );
    }

    #[test]
    fn email_detection() {
        let matches =
            find_code_matches("Contact us at test@example.com please", DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "Email Address");
    }

    #[test]
    fn credit_card_detection() {
        let cc = "Swipe card 4111-1111-1111-1111 now";
        let matches = find_code_matches(cc, DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "Credit Card (Visa/MC/AmEx)");
    }

    #[test]
    fn phone_detection() {
        let phone = "Call (555) 123-4567 today";
        let matches = find_code_matches(phone, DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "Phone Number (US)");
    }

    #[test]
    fn date_detection() {
        let d = "Today is 12/31/2023";
        let matches = find_code_matches(d, DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "Date (MM/DD/YYYY)");
    }

    #[test]
    fn privacy_act_detection() {
        let txt = "Exempt under 5 U.S.C. § 552a";
        let matches = find_code_matches(txt, DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "U.S. Privacy Act Exemption");
    }

    #[test]
    fn foia_detection() {
        let txt = "Redacted under 5 U.S.C. § 552(b)(6)";
        let matches = find_code_matches(txt, DEFAULT_CODE_SETS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].2, "FOIA Exemption");
    }

    #[test]
    fn no_match_when_pattern_absent() {
        let matches = find_code_matches("Hello world no sensitive data here", DEFAULT_CODE_SETS);
        assert!(matches.is_empty());
    }
}
