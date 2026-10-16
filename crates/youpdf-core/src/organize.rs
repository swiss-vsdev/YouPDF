//! Organisation des pages : rotation, suppression, insertion, extraction,
//! découpe et nettoyage des métadonnées.
//!
//! Toutes les opérations travaillent sur l'arbre des pages `/Pages` du
//! catalogue : les références sont retirées ou ajoutées dans les tableaux
//! `/Kids`, et les compteurs `/Count` sont recalculés en remontant.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Object, ObjectId, Stream};
use youpdf_shared::{Error, Result};

use crate::document::Document;

impl Document {
    /// Fait pivoter les pages indiquées de `delta` degrés (90 = horaire).
    pub fn rotate_pages(&mut self, pages: &[usize], delta: i64) -> Result<usize> {
        if delta % 90 != 0 {
            return Err(Error::Pdf("rotation multiple de 90° attendue".into()));
        }
        let mut rotated = 0usize;
        for number in pages {
            let Ok(id) = self.page_id(*number as u32) else {
                continue;
            };
            let Ok(dict) = self.inner.get_dictionary_mut(id) else {
                continue;
            };
            let current = dict
                .get(b"Rotate")
                .and_then(Object::as_i64)
                .unwrap_or(0);
            let angle = (((current + delta) % 360) + 360) % 360;
            dict.set("Rotate", Object::Integer(angle));
            rotated += 1;
        }
        if rotated > 0 {
            self.touch();
        }
        Ok(rotated)
    }

    /// Supprime les pages indiquées (numéros 1-indexés).
    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<usize> {
        let existing = self.inner.get_pages();
        let doomed: HashSet<ObjectId> = pages
            .iter()
            .filter_map(|number| existing.get(&(*number as u32)).copied())
            .collect();
        if doomed.is_empty() {
            return Ok(0);
        }
        if doomed.len() >= existing.len() {
            return Err(Error::Pdf(
                "au moins une page doit rester dans le document".into(),
            ));
        }
        let root = self.pages_root_id()?;
        self.prune_node(root, &doomed)?;
        for id in &doomed {
            self.inner.objects.remove(id);
        }
        self.touch();
        Ok(doomed.len())
    }

    /// Recalcule `/Count` et retire les références condamnées d'un nœud.
    fn prune_node(&mut self, node_id: ObjectId, doomed: &HashSet<ObjectId>) -> Result<usize> {
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

    /// Recalcule les compteurs de l'arbre des pages.
    pub fn refresh_page_tree(&mut self) -> Result<usize> {
        let root = self.pages_root_id()?;
        let count = self.prune_node(root, &HashSet::new())?;
        self.touch();
        Ok(count)
    }

    /// Insère les pages d'un autre PDF après la page `after`
    /// (`after == 0` place les pages en tête, `after >= page_count` en fin).
    pub fn insert_pages_after(&mut self, after: usize, source: &Path) -> Result<usize> {
        let foreign = lopdf::Document::load(source)
            .map_err(|e| Error::Pdf(format!("{} : {e}", source.display())))?;
        let numbers: Vec<u32> = foreign.get_pages().keys().copied().collect();
        if numbers.is_empty() {
            return Err(Error::Pdf(format!(
                "{} ne contient aucune page",
                source.display()
            )));
        }

        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut copied: Vec<ObjectId> = Vec::with_capacity(numbers.len());
        for number in &numbers {
            let Some(page_id) = foreign.get_pages().get(number).copied() else {
                continue;
            };
            copied.push(self.copy_foreign_page(&foreign, page_id, &mut map)?);
        }

        let root = self.pages_root_id()?;

        // `after >= page_count` veut dire « à la fin ». Viser le début
        // collait les pages en tête, à l'opposé de la demande.
        let page_count = self.inner.get_pages().len();
        let anchor = if after == 0 {
            None
        } else if after >= page_count {
            self.inner.get_pages().get(&(page_count as u32)).copied()
        } else {
            self.inner.get_pages().get(&(after as u32)).copied()
        };
        let inserted = self.insert_kids(root, anchor, &copied)?;
        self.refresh_page_tree()?;
        Ok(inserted)
    }

    /// Insère des références dans le tableau `/Kids` qui contient `anchor`.
    fn insert_kids(
        &mut self,
        node_id: ObjectId,
        anchor: Option<ObjectId>,
        new_pages: &[ObjectId],
    ) -> Result<usize> {
        let children: Vec<Object> = self
            .inner
            .get_dictionary(node_id)
            .ok()
            .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
            .unwrap_or_default();

        if anchor.is_none() {
            if let Object::Dictionary(dict) = self
                .inner
                .get_object_mut(node_id)
                .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?
            {
                let mut kids: Vec<Object> = new_pages
                    .iter()
                    .map(|id| Object::Reference(*id))
                    .collect();
                kids.extend(children);
                dict.set("Kids", Object::Array(kids));
            }
            self.set_pages_parent(node_id, new_pages);
            return Ok(new_pages.len());
        }

        let anchor_id = anchor.unwrap();
        let position = children.iter().position(|child| match child {
            Object::Reference(id) => *id == anchor_id,
            _ => false,
        });
        if let Some(position) = position {
            let mut kids = children;
            let tail = kids.split_off(position + 1);
            kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
            kids.extend(tail);
            if let Object::Dictionary(dict) = self
                .inner
                .get_object_mut(node_id)
                .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?
            {
                dict.set("Kids", Object::Array(kids));
            }
            self.set_pages_parent(node_id, new_pages);
            return Ok(new_pages.len());
        }

        // L'ancre est plus bas : descendre dans les sous-arbres.
        for child in children {
            let Object::Reference(child_id) = child else {
                continue;
            };
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
            if !is_branch {
                continue;
            }
            if self.insert_kids(child_id, anchor, new_pages)? > 0 {
                return Ok(new_pages.len());
            }
        }
        // Ancre absente : ajouter en fin de nœud racine.
        if let Object::Dictionary(dict) = self
            .inner
            .get_object_mut(node_id)
            .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?
        {
            let mut kids = dict.get(b"Kids").and_then(Object::as_array).ok().cloned().unwrap_or_default();
            kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
            dict.set("Kids", Object::Array(kids));
        }
        self.set_pages_parent(node_id, new_pages);
        Ok(new_pages.len())
    }

    /// Rattache des pages au nœud qui les reçoit.
    ///
    /// Un `/Parent` figé sur la racine alors que les pages descendent dans un
    /// sous-arbre rend l'arbre incohérent : les lecteurs qui suivent
    /// `/Parent` pour résoudre les boîtes héritées lisent le mauvais nœud.
    fn set_pages_parent(&mut self, node_id: ObjectId, new_pages: &[ObjectId]) {
        for id in new_pages {
            if let Ok(Object::Dictionary(dict)) = self.inner.get_object_mut(*id) {
                dict.set("Parent", Object::Reference(node_id));
            }
        }
    }

    /// Copie profonde d'une page étrangère, identifiants remappés.
    fn copy_foreign_page(
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
    fn copy_foreign_object(
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
    fn copy_foreign_dict(
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

    /// Écrit une copie du document ne contenant que `pages` (1-indexées).
    pub fn extract_pages(&self, pages: &[usize], path: &Path) -> Result<()> {
        let mut copy = Document::from_lopdf(self.inner.clone());
        let keep: HashSet<usize> = pages.iter().copied().collect();
        let remove: Vec<usize> = (1..=copy.page_count())
            .filter(|number| !keep.contains(number))
            .collect();
        if !remove.is_empty() {
            copy.delete_pages(&remove)?;
        }
        copy.save(path)
    }

    /// Découpe le document en fichiers de `per_file` pages maximum.
    pub fn split(&self, directory: &Path, per_file: usize) -> Result<Vec<PathBuf>> {
        let per_file = per_file.max(1);
        let total = self.page_count();
        if total == 0 {
            return Err(Error::Pdf("document vide".into()));
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "document".to_owned());
        let mut written = Vec::new();
        let mut start = 1usize;
        while start <= total {
            let end = (start + per_file - 1).min(total);
            let pages: Vec<usize> = (start..=end).collect();
            let name = format!("{stem} p{start}-{end}.pdf");
            let target = directory.join(name);
            self.extract_pages(&pages, &target)?;
            written.push(target);
            start = end + 1;
        }
        Ok(written)
    }

    /// Retire métadonnées, XMP, JavaScript et actions automatiques.
    ///
    /// Les pièces jointes et les destinations nommées ne font pas partie du
    /// lot : elles vivent elles aussi sous `/Names`, et les supprimer ferait
    /// disparaître des fichiers joints sans rapport avec le nettoyage.
    pub fn sanitize(&mut self) -> Result<usize> {
        let mut removed = 0usize;

        if let Ok(catalog) = self.inner.catalog_mut() {
            for key in [
                b"Metadata".as_slice(),
                b"OpenAction".as_slice(),
                b"AA".as_slice(),
            ] {
                if catalog.remove(key).is_some() {
                    removed += 1;
                }
            }
        }

        // Le JavaScript vit sous `/Names`, à côté de ce qu'on garde.
        let names_id = self
            .inner
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"Names").ok())
            .and_then(|names| names.as_reference().ok());
        match names_id {
            Some(id) => {
                if let Ok(names) = self.inner.get_dictionary_mut(id) {
                    if names.remove(b"JavaScript").is_some() {
                        removed += 1;
                    }
                }
            }
            None => {
                if let Ok(catalog) = self.inner.catalog_mut() {
                    if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
                        if names.remove(b"JavaScript").is_some() {
                            removed += 1;
                        }
                    }
                }
            }
        }

        if let Some(info) = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|object| Object::as_reference(object).ok())
        {
            if let Ok(dict) = self.inner.get_dictionary_mut(info) {
                let keys: Vec<Vec<u8>> = dict.iter().map(|(key, _)| key.clone()).collect();
                for key in keys {
                    if key.as_slice() == b"Producer" {
                        continue;
                    }
                    dict.remove(&key);
                    removed += 1;
                }
            }
        }

        self.touch();
        Ok(removed)
    }

    /// Table des matières brute : identifiants de page dans l'ordre.
    pub fn page_ids(&self) -> BTreeMap<u32, ObjectId> {
        self.inner.get_pages()
    }
}
