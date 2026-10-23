//! Structure du document : signets (plan) et pièces jointes.
//!
//! Lecture, écriture du plan (le `/Outlines` du catalogue) et lecture des
//! pièces jointes.

pub mod attachments;
pub mod headings;
pub mod outline;

pub use attachments::AttachmentInfo;
pub use headings::OutlineStyle;
pub use outline::{OutlineEntry, OutlineItem, OutlineTarget};

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;

/// Plafond de sécurité sur la profondeur du plan.
pub(crate) const MAX_OUTLINE_DEPTH: usize = 24;

impl Document {
    /// Entrée de dictionnaire déréférencée.
    pub(crate) fn dict_entry<'a>(&'a self, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
        let value = dict.get(key).ok()?;
        match value {
            Object::Dictionary(inner) => Some(inner),
            Object::Reference(id) => self.inner.get_dictionary(*id).ok(),
            _ => None,
        }
    }

    /// Identifiant d'objet référencé par `object` (ou l'objet lui-même s'il
    /// est déjà une référence valide).
    pub(crate) fn resolve_id(&self, object: &Object) -> Option<ObjectId> {
        match object {
            Object::Reference(id) => Some(*id),
            _ => None,
        }
    }

    /// Retire du magasin les objets devenus inatteignables.
    ///
    /// Le balayage part des racines du trailer : ce qui a été remplacé n'y
    /// est plus. Un objet que des références vivantes partagent avec l'ancien
    /// (destination nommée, lien vers un signet, pièce jointe réutilisée)
    /// reste donc atteint et survit.
    pub(crate) fn purge_unreachable(&mut self, doomed: &[ObjectId]) {
        if doomed.is_empty() {
            return;
        }
        let doomed_set: std::collections::HashSet<ObjectId> = doomed.iter().copied().collect();
        let mut stack: Vec<ObjectId> = self
            .inner
            .trailer
            .iter()
            .filter_map(|(_, value)| value.as_reference().ok())
            .collect();
        let mut reachable: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        while let Some(id) = stack.pop() {
            if !reachable.insert(id) {
                continue;
            }
            let Some(object) = self.inner.objects.get(&id) else {
                continue;
            };
            let mut refs = std::collections::HashSet::new();
            collect_references(object, &mut refs);
            stack.extend(refs);
        }
        for id in &doomed_set {
            if !reachable.contains(id) {
                self.inner.objects.remove(id);
            }
        }
    }
}

/// Rassemble les identifiants d'objets référencés par `object`, en profondeur.
pub(crate) fn collect_references(object: &Object, out: &mut std::collections::HashSet<ObjectId>) {
    match object {
        Object::Reference(id) => {
            out.insert(*id);
        }
        Object::Array(items) => {
            for item in items {
                collect_references(item, out);
            }
        }
        Object::Dictionary(dict) => {
            for (_, value) in dict.iter() {
                collect_references(value, out);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                collect_references(value, out);
            }
        }
        _ => {}
    }
}

/// Décode une chaîne PDF : UTF-16BE avec BOM, sinon Latin-1/ASCII.
pub fn decode_pdf_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    // Beaucoup de producteurs écrivent de l'UTF-8 brut : on le préfère,
    // sinon on retombe sur Latin-1 (octet = caractère).
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    bytes
        .iter()
        .map(|byte| match byte {
            0x0A | 0x0D | 0x09 => ' ',
            _ => *byte as char,
        })
        .collect()
}
