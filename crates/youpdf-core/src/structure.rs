//! Structure du document : signets (plan) et pièces jointes.
//!
//! Lecture seule : ces deux structures viennent du catalogue PDF et ne sont
//! pas modifiées par l'application.

use lopdf::{Dictionary, Object, ObjectId};

use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Entrée du plan (signets) d'un document.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineItem {
    /// Titre affiché.
    pub title: String,
    /// Page cible (1-indexée) si la destination est résolue.
    pub page: Option<usize>,
    /// Profondeur (0 = racine).
    pub level: usize,
}

/// Fichier embarqué dans le document.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentInfo {
    /// Nom du fichier.
    pub name: String,
    /// Taille en octets, si connue.
    pub size: Option<usize>,
    /// Description éventuelle.
    pub description: Option<String>,
}

/// Plafond de sécurité sur la profondeur du plan.
const MAX_OUTLINE_DEPTH: usize = 24;

impl Document {
    /// Plan du document (signets), dans l'ordre d'affichage.
    pub fn outline(&self) -> Vec<OutlineItem> {
        let mut items = Vec::new();
        let Ok(catalog) = self.inner.catalog() else {
            return items;
        };
        let Some(outlines) = self.dict_entry(catalog, b"Outlines") else {
            return items;
        };
        let Some(first) = outlines.get(b"First").ok().and_then(|o| self.resolve_id(o)) else {
            return items;
        };
        let pages = self.inner.get_pages();
        let mut stack = vec![(first, 0usize)];
        let mut guard = 0usize;
        while let Some((node_id, level)) = stack.pop() {
            guard += 1;
            if guard > 20_000 {
                break;
            }
            let Ok(node) = self.inner.get_dictionary(node_id) else {
                continue;
            };
            if let Ok(title) = node.get(b"Title").and_then(Object::as_str) {
                let title = decode_pdf_text(title);
                if !title.trim().is_empty() {
                    items.push(OutlineItem {
                        title,
                        page: self.outline_page(node, &pages),
                        level: level.min(MAX_OUTLINE_DEPTH),
                    });
                }
            }
            if level < MAX_OUTLINE_DEPTH {
                if let Some(child) = node.get(b"First").ok().and_then(|o| self.resolve_id(o)) {
                    stack.push((child, level + 1));
                }
            }
            if let Some(next) = node.get(b"Next").ok().and_then(|o| self.resolve_id(o)) {
                stack.push((next, level));
            }
        }
        items
    }

    /// Page visée par une entrée de plan (`/Dest` ou `/A /D`).
    fn outline_page(
        &self,
        node: &Dictionary,
        pages: &std::collections::BTreeMap<u32, ObjectId>,
    ) -> Option<usize> {
        let dest = match node.get(b"Dest") {
            Ok(dest) => Some(dest),
            Err(_) => self
                .dict_entry(node, b"A")
                .and_then(|action| action.get(b"D").ok()),
        }?;
        let target = match dest {
            Object::Array(items) => items.first()?,
            Object::Name(_) => return None,
            other => other,
        };
        let (target_id, target) = self.inner.dereference(target).ok()?;
        let page_id = match target_id {
            Some(id) => id,
            None => match target {
                Object::Dictionary(dict) => dict.get(b"Page").ok().and_then(|o| self.resolve_id(o))?,
                _ => return None,
            },
        };
        pages
            .iter()
            .find(|(_, id)| **id == page_id)
            .map(|(number, _)| *number as usize)
    }

    /// Pièces jointes déclarées dans `/Names /EmbeddedFiles`.
    pub fn attachments(&self) -> Vec<AttachmentInfo> {
        let mut files = Vec::new();
        let Ok(catalog) = self.inner.catalog() else {
            return files;
        };
        let Some(names) = self.dict_entry(catalog, b"Names") else {
            return files;
        };
        let Some(tree) = self.dict_entry(names, b"EmbeddedFiles") else {
            return files;
        };
        let mut nodes = vec![tree.clone()];
        let mut guard = 0usize;
        while let Some(node) = nodes.pop() {
            guard += 1;
            if guard > 1_000 {
                break;
            }
            if let Ok(entries) = node.get(b"Names").and_then(Object::as_array) {
                for pair in entries.chunks(2) {
                    let Some(spec) = self.dict_object(&pair[1]) else {
                        continue;
                    };
                    if let Some(info) = self.describe_attachment(&spec) {
                        files.push(info);
                    }
                }
            }
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
                    if let Some(id) = self.resolve_id(kid) {
                        if let Ok(child) = self.inner.get_dictionary(id) {
                            nodes.push(child.clone());
                        }
                    }
                }
            }
        }
        files
    }

    /// Dictionnaire d'un objet, en suivant une référence.
    fn dict_object(&self, object: &Object) -> Option<Dictionary> {
        match object {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner.get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Lit un descripteur de fichier embarqué.
    fn describe_attachment(&self, filespec: &Dictionary) -> Option<AttachmentInfo> {
        let name = filespec
            .get(b"UF")
            .or_else(|_| filespec.get(b"F"))
            .ok()
            .and_then(object_text)
            .unwrap_or_else(|| "attachment".to_owned());
        let description = filespec.get(b"Desc").ok().and_then(object_text);
        let size = self
            .dict_entry(filespec, b"EF")
            .and_then(|ef| ef.get(b"UF").or_else(|_| ef.get(b"F")).ok())
            .and_then(|obj| self.resolve_id(obj))
            .and_then(|id| self.inner.get_object(id).ok())
            .and_then(|obj| match obj {
                Object::Stream(stream) => {
                    let params_size = self
                        .dict_entry(&stream.dict, b"Params")
                        .and_then(|params| params.get(b"Size").ok())
                        .and_then(|size| size.as_i64().ok())
                        .map(|size| size as usize);
                    Some(
                        params_size.unwrap_or_else(|| {
                            stream
                                .decompressed_content()
                                .map(|bytes| bytes.len())
                                .unwrap_or(stream.content.len())
                        }),
                    )
                }
                _ => None,
            });
        Some(AttachmentInfo {
            name,
            size,
            description,
        })
    }

    /// Contenu d'une pièce jointe, dans l'ordre d'`attachments`.
    pub fn attachment_bytes(&self, index: usize) -> Result<Vec<u8>> {
        let catalog = self
            .inner
            .catalog()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        let names = self
            .dict_entry(catalog, b"Names")
            .ok_or_else(|| Error::NotFound("pièces jointes absentes".into()))?;
        let tree = self
            .dict_entry(names, b"EmbeddedFiles")
            .ok_or_else(|| Error::NotFound("pièces jointes absentes".into()))?;

        let mut streams: Vec<ObjectId> = Vec::new();
        let mut nodes = vec![tree.clone()];
        while let Some(node) = nodes.pop() {
            if let Ok(entries) = node.get(b"Names").and_then(Object::as_array) {
                for pair in entries.chunks(2) {
                    let Some(spec) = self.dict_object(&pair[1]) else {
                        continue;
                    };
                    if let Some(id) = self
                        .dict_entry(&spec, b"EF")
                        .and_then(|ef| ef.get(b"UF").or_else(|_| ef.get(b"F")).ok())
                        .and_then(|obj| self.resolve_id(obj))
                    {
                        streams.push(id);
                    }
                }
            }
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
                    if let Some(id) = self.resolve_id(kid) {
                        if let Ok(child) = self.inner.get_dictionary(id) {
                            nodes.push(child.clone());
                        }
                    }
                }
            }
        }

        let id = *streams
            .get(index)
            .ok_or_else(|| Error::NotFound(format!("pièce jointe {index} inexistante")))?;
        let stream = match self.inner.get_object(id) {
            Ok(Object::Stream(stream)) => stream,
            _ => {
                return Err(Error::Pdf(format!(
                    "pièce jointe {index} : flux introuvable"
                )))
            }
        };
        // Certains producteurs omettent `/Filter` ou `/Length` : dans ce cas
        // le contenu brut est déjà le fichier.
        Ok(stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()))
    }

    /// Entrée de dictionnaire déréférencée.
    fn dict_entry<'a>(&'a self, dict: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
        let value = dict.get(key).ok()?;
        match value {
            Object::Dictionary(inner) => Some(inner),
            Object::Reference(id) => self.inner.get_dictionary(*id).ok(),
            _ => None,
        }
    }

    /// Identifiant d'objet référencé par `object` (ou l'objet lui-même s'il
    /// est déjà une référence valide).
    fn resolve_id(&self, object: &Object) -> Option<ObjectId> {
        match object {
            Object::Reference(id) => Some(*id),
            _ => None,
        }
    }
}

/// Texte d'un objet nom ou chaîne PDF.
fn object_text(object: &Object) -> Option<String> {
    match object {
        Object::String(bytes, _) => Some(decode_pdf_text(bytes)),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
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
    bytes
        .iter()
        .map(|byte| match byte {
            0x0A | 0x0D | 0x09 => ' ',
            _ => *byte as char,
        })
        .collect()
}
