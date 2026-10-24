//! Helpers de l'arbre des pages.

use std::collections::HashSet;

use lopdf::{Object, ObjectId};

use youpdf_shared::{Error, Result};

use crate::document::Document;

impl Document {
    /// Recalcule `/Count` et retire les références condamnées d'un nœud.
    pub(crate) fn prune_node(&mut self, node_id: ObjectId, doomed: &HashSet<ObjectId>) -> Result<usize> {
        let children: Vec<Object> = self
            .inner
            .get_dictionary(node_id)
            .ok()
            .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
            .unwrap_or_default();

        let mut kept: Vec<Object> = Vec::with_capacity(children.len());
        let mut count = 0usize;
        for child in children {
            let Object::Reference(child_id) = child else {
                kept.push(child);
                count += 1;
                continue;
            };
            if doomed.contains(&child_id) {
                continue;
            }
            let is_branch = self
                .inner
                .get_dictionary(child_id)
                .map(|dict| {
                    dict.get(b"Type")
                        .and_then(Object::as_name)
                        .map(|name| name == b"Pages")
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if is_branch {
                let sub = self.prune_node(child_id, doomed)?;
                if sub == 0 {
                    continue;
                }
                count += sub;
            } else {
                count += 1;
            }
            kept.push(child);
        }

        let dict = self
            .inner
            .get_dictionary_mut(node_id)
            .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?;
        dict.set("Kids", Object::Array(kept));
        dict.set("Count", Object::Integer(count as i64));
        Ok(count)
    }

    /// Enfants `/Kids` d'un nœud, ou tableau vide.
    pub(crate) fn kids_of(&self, node_id: ObjectId) -> Vec<Object> {
        self.inner
            .get_dictionary(node_id)
            .ok()
            .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
            .unwrap_or_default()
    }

    /// Écrit le tableau `/Kids` d'un nœud.
    pub(crate) fn set_kids(&mut self, node_id: ObjectId, kids: Vec<Object>) -> Result<()> {
        let dict = self
            .inner
            .get_dictionary_mut(node_id)
            .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?;
        dict.set("Kids", Object::Array(kids));
        Ok(())
    }

    /// Vrai si le nœud est un nœud intermédiaire `/Pages`.
    pub(crate) fn is_branch(&self, node_id: ObjectId) -> bool {
        self.inner
            .get_dictionary(node_id)
            .map(|dict| {
                dict.get(b"Type")
                    .and_then(Object::as_name)
                    .map(|name| name == b"Pages")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Nœud dont le tableau `/Kids` contient directement cette page.
    pub(crate) fn kids_node_of(&self, page_id: ObjectId) -> Option<ObjectId> {
        let root = self.pages_root_id().ok()?;
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            for child in self.kids_of(node_id) {
                let Object::Reference(child_id) = child else {
                    continue;
                };
                if child_id == page_id {
                    return Some(node_id);
                }
                if self.is_branch(child_id) {
                    stack.push(child_id);
                }
            }
        }
        None
    }

    /// Rattache des pages au nœud qui les reçoit.
    ///
    /// Un `/Parent` figé sur la racine alors que les pages descendent dans un
    /// sous-arbre rend l'arbre incohérent : les lecteurs qui suivent
    /// `/Parent` pour résoudre les boîtes héritées lisent le mauvais nœud.
    pub(crate) fn set_pages_parent(&mut self, node_id: ObjectId, new_pages: &[ObjectId]) {
        for id in new_pages {
            if let Ok(Object::Dictionary(dict)) = self.inner.get_object_mut(*id) {
                dict.set("Parent", Object::Reference(node_id));
            }
        }
    }
}
