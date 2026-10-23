use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};
use youpdf_shared::{Error, Result};
use crate::document::Document;
use super::decode_pdf_text;

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

impl Document {
    /// Pièces jointes déclarées dans `/Names /EmbeddedFiles`.
    pub fn attachments(&self) -> Vec<AttachmentInfo> {
        self.embedded_file_entries()
            .iter()
            .filter_map(|(name, value)| {
                let mut info = self.describe_attachment(&self.dict_object(value)?)?;
                info.name = name.clone();
                Some(info)
            })
            .collect()
    }

    pub(crate) fn embedded_file_entries(&self) -> Vec<(String, Object)> {
        let mut entries = Vec::new();
        let Ok(catalog) = self.inner.catalog() else {
            return entries;
        };
        let Some(names) = self.dict_entry(catalog, b"Names") else {
            return entries;
        };
        let Some(tree) = self.dict_entry(names, b"EmbeddedFiles") else {
            return entries;
        };
        let mut nodes = vec![tree.clone()];
        let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        if let Ok(root_id) = names.get(b"EmbeddedFiles").and_then(Object::as_reference) {
            seen.insert(root_id);
        }
        while let Some(node) = nodes.pop() {
            if let Ok(pairs) = node.get(b"Names").and_then(Object::as_array) {
                for pair in pairs.chunks(2) {
                    let (Some(key), Some(value)) = (pair.first(), pair.get(1)) else {
                        continue;
                    };
                    let Some(filespec) = self.dict_object(value) else {
                        continue;
                    };
                    let name = filespec
                        .get(b"UF")
                        .or_else(|_| filespec.get(b"F"))
                        .ok()
                        .and_then(object_text)
                        .or_else(|| object_text(key))
                        .unwrap_or_else(|| "attachment".to_owned());
                    entries.push((name, value.clone()));
                }
            }
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids.iter().rev() {
                    if let Some(id) = self.resolve_id(kid) {
                        if !seen.insert(id) {
                            continue;
                        }
                        if let Ok(child) = self.inner.get_dictionary(id) {
                            nodes.push(child.clone());
                        }
                    }
                }
            }
        }
        entries
    }

    pub(crate) fn attachment_object_ids(&self, value: &Object) -> Vec<ObjectId> {
        let mut ids = Vec::new();
        if let Ok(id) = value.as_reference() {
            ids.push(id);
        }
        if let Some(filespec) = self.dict_object(value) {
            if let Some(ef) = self.dict_entry(&filespec, b"EF") {
                for key in [b"F".as_slice(), b"UF".as_slice()] {
                    if let Ok(id) = ef.get(key).and_then(Object::as_reference) {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    pub(crate) fn dict_object(&self, object: &Object) -> Option<Dictionary> {
        match object {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner.get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    pub(crate) fn describe_attachment(&self, filespec: &Dictionary) -> Option<AttachmentInfo> {
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
        let entries = self.embedded_file_entries();
        let Some((_, value)) = entries.get(index) else {
            return Err(Error::NotFound(format!(
                "pièce jointe {index} inexistante"
            )));
        };
        let filespec = self.dict_object(value).ok_or_else(|| {
            Error::Pdf(format!("pièce jointe {index} : descripteur illisible"))
        })?;
        let stream_id = self
            .dict_entry(&filespec, b"EF")
            .and_then(|ef| ef.get(b"UF").or_else(|_| ef.get(b"F")).ok())
            .and_then(|obj| self.resolve_id(obj))
            .ok_or_else(|| Error::Pdf(format!("pièce jointe {index} : flux introuvable")))?;
        let stream = match self.inner.get_object(stream_id) {
            Ok(Object::Stream(stream)) => stream,
            _ => {
                return Err(Error::Pdf(format!(
                    "pièce jointe {index} : flux introuvable"
                )))
            }
        };
        Ok(stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()))
    }

    /// Ajoute une pièce jointe embarquée, ou remplace celle du même nom.
    pub fn add_attachment(
        &mut self,
        name: &str,
        description: Option<&str>,
        bytes: &[u8],
    ) -> Result<()> {
        let file_name = name.trim();
        let file_name = if file_name.is_empty() {
            "attachment"
        } else {
            file_name
        };

        let stream_id = self.inner_mut().add_object(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"EmbeddedFile".to_vec()),
                "Subtype" => Object::Name(b"application/octet-stream".to_vec()),
                "Params" => Object::Dictionary(dictionary! {
                    "Size" => Object::Integer(bytes.len() as i64),
                }),
            },
            bytes.to_vec(),
        ));
        let mut filespec = dictionary! {
            "Type" => Object::Name(b"Filespec".to_vec()),
            "F" => Object::string_literal(file_name),
            "UF" => Object::string_literal(file_name),
        };
        if let Some(description) = description.map(str::trim).filter(|d| !d.is_empty()) {
            filespec.set("Desc", Object::string_literal(description));
        }
        let mut ef = Dictionary::new();
        ef.set("F", Object::Reference(stream_id));
        ef.set("UF", Object::Reference(stream_id));
        filespec.set("EF", Object::Dictionary(ef));
        let spec_id = self.inner_mut().add_object(Object::Dictionary(filespec));

        let mut entries: Vec<(String, Object)> = Vec::new();
        let mut doomed = self.embedded_tree_ids();
        let mut replaced = false;
        for (existing, value) in self.embedded_file_entries() {
            if existing == file_name {
                doomed.extend(self.attachment_object_ids(&value));
                if !replaced {
                    entries.push((existing, Object::Reference(spec_id)));
                    replaced = true;
                }
                continue;
            }
            entries.push((existing, value));
        }
        if !replaced {
            entries.push((file_name.to_owned(), Object::Reference(spec_id)));
        }

        self.write_embedded_tree(&entries)?;
        self.purge_unreachable(&doomed);
        self.touch();
        Ok(())
    }

    /// Retire une pièce jointe, dans l'ordre d'`attachments`.
    pub fn remove_attachment(&mut self, index: usize) -> Result<()> {
        let entries = self.embedded_file_entries();
        let Some((_, removed)) = entries.get(index).cloned() else {
            return Err(Error::NotFound(format!(
                "pièce jointe {index} inexistante"
            )));
        };
        let kept: Vec<(String, Object)> = entries
            .into_iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, entry)| entry)
            .collect();

        let mut doomed = self.embedded_tree_ids();
        doomed.extend(self.attachment_object_ids(&removed));

        if kept.is_empty() {
            self.drop_embedded_files();
        } else {
            self.write_embedded_tree(&kept)?;
        }
        self.purge_unreachable(&doomed);
        self.touch();
        Ok(())
    }

    pub(crate) fn embedded_tree_ids(&self) -> Vec<ObjectId> {
        let mut ids = Vec::new();
        let Ok(catalog) = self.inner.catalog() else {
            return ids;
        };
        let Some(names) = self.dict_entry(catalog, b"Names") else {
            return ids;
        };
        let Ok(tree) = names.get(b"EmbeddedFiles") else {
            return ids;
        };
        let mut stack: Vec<ObjectId> = Vec::new();
        if let Ok(root) = tree.as_reference() {
            ids.push(root);
            stack.push(root);
        } else if let Ok(dict) = tree.as_dict() {
            if let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
                    if let Ok(kid_id) = kid.as_reference() {
                        if !ids.contains(&kid_id) {
                            ids.push(kid_id);
                            stack.push(kid_id);
                        }
                    }
                }
            }
        }
        while let Some(id) = stack.pop() {
            let Ok(node) = self.inner.get_dictionary(id) else {
                continue;
            };
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
                    if let Ok(kid_id) = kid.as_reference() {
                        if !ids.contains(&kid_id) {
                            ids.push(kid_id);
                            stack.push(kid_id);
                        }
                    }
                }
            }
        }
        ids
    }

    pub(crate) fn write_embedded_tree(&mut self, entries: &[(String, Object)]) -> Result<()> {
        let mut names: Vec<Object> = Vec::with_capacity(entries.len() * 2);
        for (name, filespec) in entries {
            names.push(Object::string_literal(name.as_str()));
            names.push(filespec.clone());
        }
        let leaf = self.inner_mut().add_object(Object::Dictionary(dictionary! {
            "Names" => Object::Array(names),
        }));

        let names_id = self
            .inner_mut()
            .catalog_mut()
            .ok()
            .and_then(|catalog| match catalog.get(b"Names") {
                Ok(Object::Reference(id)) => Some(*id),
                _ => None,
            });
        if let Some(id) = names_id {
            let dict = self
                .inner_mut()
                .get_object_mut(id)
                .map_err(|e| Error::Pdf(format!("arbre des pièces jointes : {e}")))?
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("arbre des pièces jointes : {e}")))?;
            dict.set("EmbeddedFiles", Object::Reference(leaf));
            return Ok(());
        }

        let catalog = self
            .inner_mut()
            .catalog_mut()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        match catalog.get_mut(b"Names") {
            Ok(Object::Dictionary(names)) => names.set("EmbeddedFiles", Object::Reference(leaf)),
            _ => {
                let mut fresh = Dictionary::new();
                fresh.set("EmbeddedFiles", Object::Reference(leaf));
                catalog.set("Names", Object::Dictionary(fresh));
            }
        }
        Ok(())
    }

    pub(crate) fn drop_embedded_files(&mut self) {
        let names_id = self
            .inner_mut()
            .catalog_mut()
            .ok()
            .and_then(|catalog| match catalog.get(b"Names") {
                Ok(Object::Reference(id)) => Some(*id),
                _ => None,
            });
        if let Some(id) = names_id {
            if let Ok(dict) = self
                .inner_mut()
                .get_object_mut(id)
                .and_then(|object| object.as_dict_mut())
            {
                dict.remove(b"EmbeddedFiles");
            }
            return;
        }
        if let Ok(catalog) = self.inner_mut().catalog_mut() {
            if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
                names.remove(b"EmbeddedFiles");
            }
        }
    }
}

pub(crate) fn object_text(object: &Object) -> Option<String> {
    match object {
        Object::String(bytes, _) => Some(decode_pdf_text(bytes)),
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}