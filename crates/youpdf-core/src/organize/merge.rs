//! Copie d'objets depuis un document étranger (insertion / remplacement de pages).

use std::collections::HashMap;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;

impl Document {
    /// Copie profonde d'une page étrangère, identifiants remappés.
    pub(crate) fn copy_foreign_page(
        &mut self,
        foreign: &lopdf::Document,
        page_id: ObjectId,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<ObjectId> {
        if let Some(new_id) = map.get(&page_id) {
            return Ok(*new_id);
        }
        let placeholder = self.inner.add_object(Object::Null);
        map.insert(page_id, placeholder);
        let object = foreign
            .get_object(page_id)
            .map_err(|e| Error::Pdf(format!("page source : {e}")))?
            .clone();
        let copied = self.copy_foreign_object(foreign, &object, map)?;
        *self
            .inner
            .get_object_mut(placeholder)
            .map_err(|e| Error::Pdf(format!("copie : {e}")))? = copied;
        Ok(placeholder)
    }

    /// Copie profonde d'un objet, en suivant les références.
    pub(crate) fn copy_foreign_object(
        &mut self,
        foreign: &lopdf::Document,
        object: &Object,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<Object> {
        Ok(match object {
            Object::Reference(id) => {
                Object::Reference(self.copy_foreign_page(foreign, *id, map)?)
            }
            Object::Array(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for item in items {
                    copied.push(self.copy_foreign_object(foreign, item, map)?);
                }
                Object::Array(copied)
            }
            Object::Dictionary(dict) => Object::Dictionary(self.copy_foreign_dict(foreign, dict, map)?),
            Object::Stream(stream) => Object::Stream(Stream::new(
                self.copy_foreign_dict(foreign, &stream.dict, map)?,
                stream.content.clone(),
            )),
            other => other.clone(),
        })
    }

    /// Copie profonde d'un dictionnaire ; `/Parent` est retiré des pages.
    pub(crate) fn copy_foreign_dict(
        &mut self,
        foreign: &lopdf::Document,
        dict: &Dictionary,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<Dictionary> {
        let is_page = dict
            .get(b"Type")
            .and_then(Object::as_name)
            .map(|name| name == b"Page")
            .unwrap_or(false);
        let mut copied = Dictionary::new();
        for (key, value) in dict.iter() {
            if is_page && key.as_slice() == b"Parent" {
                continue;
            }
            copied.set(key.clone(), self.copy_foreign_object(foreign, value, map)?);
        }
        Ok(copied)
    }

    /// Insère des références dans le tableau `/Kids` qui contient `anchor`.
    ///
    /// Sans ancre, les pages passent en tête. Si l'ancre est absente du
    /// document, elles finissent en fin de nœud racine. La recherche descend
    /// dans **tous** les sous-arbres avant de conclure à l'absence : une ancre
    /// présente dans un second sous-arbre ne doit pas être confondue avec une
    /// ancre manquante, sinon la page atterrit dans le mauvais sous-arbre.
    pub(crate) fn insert_kids(
        &mut self,
        node_id: ObjectId,
        anchor: Option<ObjectId>,
        new_pages: &[ObjectId],
    ) -> Result<usize> {
        if let Some(anchor_id) = anchor {
            if self.insert_kids_after(node_id, anchor_id, new_pages)? {
                return Ok(new_pages.len());
            }
        } else {
            let mut kids: Vec<Object> = new_pages
                .iter()
                .map(|id| Object::Reference(*id))
                .collect();
            kids.extend(self.kids_of(node_id));
            self.set_kids(node_id, kids)?;
            self.set_pages_parent(node_id, new_pages);
            return Ok(new_pages.len());
        }

        // Ancre absente du document : ajouter en fin de nœud racine.
        let mut kids = self.kids_of(node_id);
        kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
        self.set_kids(node_id, kids)?;
        self.set_pages_parent(node_id, new_pages);
        Ok(new_pages.len())
    }

    /// Insère `new_pages` juste après `anchor_id`, dans le nœud qui le contient
    /// ou dans l'un de ses sous-arbres. Renvoie `true` si l'ancre est trouvée.
    pub(crate) fn insert_kids_after(
        &mut self,
        node_id: ObjectId,
        anchor_id: ObjectId,
        new_pages: &[ObjectId],
    ) -> Result<bool> {
        let children = self.kids_of(node_id);
        if let Some(position) = children
            .iter()
            .position(|child| matches!(child, Object::Reference(id) if *id == anchor_id))
        {
            let mut kids = children;
            let tail = kids.split_off(position + 1);
            kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
            kids.extend(tail);
            self.set_kids(node_id, kids)?;
            self.set_pages_parent(node_id, new_pages);
            return Ok(true);
        }
        for child in children {
            let Object::Reference(child_id) = child else {
                continue;
            };
            if !self.is_branch(child_id) {
                continue;
            }
            if self.insert_kids_after(child_id, anchor_id, new_pages)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
