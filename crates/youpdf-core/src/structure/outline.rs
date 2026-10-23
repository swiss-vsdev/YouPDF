use lopdf::{Dictionary, Object, ObjectId};
use youpdf_shared::{Error, Result};
use crate::document::Document;
use crate::edit::encode_pdf_literal;
use super::{decode_pdf_text, MAX_OUTLINE_DEPTH};

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

/// Ce qu'une entrée de plan vise, tel qu'écrit dans le document.
///
/// Conserver la cible d'origine permet à un renommage ou à un déplacement de
/// ne pas perdre la position de vue (`/XYZ …`) ni les actions (`/A`).
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineTarget {
    /// Aucune cible lisible.
    None,
    /// Tableau `/Dest`.
    Dest(Object),
    /// Dictionnaire `/A` (souvent un `GoTo`).
    Action(Object),
}

/// Entrée de plan, avec ses sous-entrées : la forme que l'interface édite.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    /// Titre affiché.
    pub title: String,
    /// Page visée (1-indexée), si connue.
    pub page: Option<usize>,
    /// Cible d'origine, réécrite telle quelle. `OutlineTarget::None` signifie
    /// « écrire une destination `/Fit` sur `page` ».
    pub target: OutlineTarget,
    /// Sous-entrées, dans l'ordre d'affichage.
    pub children: Vec<OutlineEntry>,
}

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
    pub(crate) fn outline_page(
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

    /// Plan du document en arbre : titres, pages visées, sous-entrées.
    ///
    /// C'est la forme éditable du plan (voir [`Document::write_outline`]),
    /// l'ordre des enfants est celui de `/First` puis `/Next`.
    pub fn outline_tree(&self) -> Vec<OutlineEntry> {
        let Some(root) = self.outline_first() else {
            return Vec::new();
        };
        let pages = self.inner.get_pages();
        let mut budget = 20_000usize;
        self.outline_siblings(root, &pages, 0, &mut budget)
    }

    /// Remplace le plan par l'arbre fourni.
    ///
    /// Une liste vide retire `/Outlines`. Une entrée dont la cible est inconnue
    /// pointe la page donnée (`/Fit`) ; une cible d'origine est reprise telle
    /// quelle, donc la position de vue et les actions survivent à un
    /// renommage ou à un déplacement.
    pub fn write_outline(&mut self, entries: &[OutlineEntry]) -> Result<usize> {
        let doomed = self.outline_object_ids();
        self.drop_outline_key();

        if entries.is_empty() {
            self.purge_unreachable(&doomed);
            self.touch();
            return Ok(0);
        }

        fn walk(
            entries: &[OutlineEntry],
            level: usize,
            rows: &mut Vec<(String, Option<usize>, OutlineTarget, usize)>,
        ) {
            for entry in entries {
                rows.push((
                    entry.title.clone(),
                    entry.page,
                    entry.target.clone(),
                    level,
                ));
                walk(&entry.children, level + 1, rows);
            }
        }
        let mut rows = Vec::new();
        walk(entries, 0, &mut rows);

        let pages = self.inner.get_pages();
        let mut item_ids: Vec<ObjectId> = Vec::with_capacity(rows.len());
        for (title, page, target, _) in &rows {
            let mut dict = Dictionary::from_iter(vec![(
                "Title",
                Object::string_literal(encode_pdf_literal(title)),
            )]);
            match target {
                OutlineTarget::Dest(object) => {
                    let alive = match object {
                        Object::Array(items) => match items.first() {
                            Some(Object::Reference(id)) => pages.values().any(|page| page == id),
                            _ => true,
                        },
                        _ => true,
                    };
                    if alive {
                        dict.set("Dest", object.clone());
                    }
                }
                OutlineTarget::Action(object) => {
                    dict.set("A", object.clone());
                }
                OutlineTarget::None => {
                    if let Some(page_id) = page.and_then(|number| pages.get(&(number as u32)).copied())
                    {
                        dict.set(
                            "Dest",
                            Object::Array(vec![
                                Object::Reference(page_id),
                                Object::Name(b"Fit".to_vec()),
                            ]),
                        );
                    }
                }
            }
            item_ids.push(self.inner_mut().add_object(Object::Dictionary(dict)));
        }

        let root_id = self
            .inner_mut()
            .add_object(Object::Dictionary(Dictionary::from_iter(vec![(
                "Type",
                Object::Name(b"Outlines".to_vec()),
            )])));

        let levels: Vec<usize> = rows.iter().map(|(_, _, _, level)| *level).collect();
        self.link_outline_levels(&levels, &item_ids, root_id)?;

        self.inner_mut()
            .catalog_mut()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .set("Outlines", Object::Reference(root_id));

        self.purge_unreachable(&doomed);
        self.touch();
        Ok(rows.len())
    }

    pub(crate) fn outline_first(&self) -> Option<ObjectId> {
        let catalog = self.inner.catalog().ok()?;
        let outlines = self.dict_entry(catalog, b"Outlines")?;
        outlines.get(b"First").ok().and_then(|o| self.resolve_id(o))
    }

    pub(crate) fn outline_siblings(
        &self,
        first: ObjectId,
        pages: &std::collections::BTreeMap<u32, ObjectId>,
        depth: usize,
        budget: &mut usize,
    ) -> Vec<OutlineEntry> {
        let mut out = Vec::new();
        if depth > MAX_OUTLINE_DEPTH {
            return out;
        }
        let mut node = first;
        loop {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let Ok(dict) = self.inner.get_dictionary(node) else {
                break;
            };
            let title = dict
                .get(b"Title")
                .and_then(Object::as_str)
                .map(decode_pdf_text)
                .unwrap_or_default();
            if !title.trim().is_empty() {
                let children = dict
                    .get(b"First")
                    .ok()
                    .and_then(|o| self.resolve_id(o))
                    .map(|child| self.outline_siblings(child, pages, depth + 1, budget))
                    .unwrap_or_default();
                out.push(OutlineEntry {
                    title,
                    page: self.outline_page(dict, pages),
                    target: self.outline_target(dict),
                    children,
                });
            }
            match dict.get(b"Next").ok().and_then(|o| self.resolve_id(o)) {
                Some(next) => node = next,
                None => break,
            }
        }
        out
    }

    pub(crate) fn outline_target(&self, node: &Dictionary) -> OutlineTarget {
        if let Ok(dest) = node.get(b"Dest") {
            if !matches!(dest, Object::Null) {
                return OutlineTarget::Dest(dest.clone());
            }
        }
        if let Some(action) = self.dict_entry(node, b"A") {
            return OutlineTarget::Action(Object::Dictionary(action.clone()));
        }
        OutlineTarget::None
    }

    pub(crate) fn outline_object_ids(&self) -> Vec<ObjectId> {
        let mut doomed = Vec::new();
        let Ok(catalog) = self.inner.catalog() else {
            return doomed;
        };
        let Some(outlines) = self.dict_entry(catalog, b"Outlines") else {
            return doomed;
        };
        if let Ok(root) = catalog.get(b"Outlines").and_then(Object::as_reference) {
            doomed.push(root);
        }
        let Some(first) = outlines.get(b"First").ok().and_then(|o| self.resolve_id(o)) else {
            return doomed;
        };
        let mut stack = vec![first];
        let mut budget = 20_000usize;
        while let Some(node) = stack.pop() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let Ok(dict) = self.inner.get_dictionary(node) else {
                continue;
            };
            doomed.push(node);
            if let Some(child) = dict.get(b"First").ok().and_then(|o| self.resolve_id(o)) {
                stack.push(child);
            }
            if let Some(next) = dict.get(b"Next").ok().and_then(|o| self.resolve_id(o)) {
                stack.push(next);
            }
        }
        doomed
    }

    /// Supprime tous les signets (`/Outlines` du catalogue).
    ///
    /// Renvoie le nombre d'entrées qui existaient avant la suppression.
    pub fn clear_outline(&mut self) -> Result<usize> {
        let count = self.outline().len();
        let doomed = self.outline_object_ids();
        self.drop_outline_key();
        self.purge_unreachable(&doomed);
        self.touch();
        Ok(count)
    }

    pub(crate) fn link_outline_levels(
        &mut self,
        levels: &[usize],
        item_ids: &[ObjectId],
        root_id: ObjectId,
    ) -> Result<()> {
        let count = levels.len();
        if count == 0 || item_ids.len() != count {
            return Ok(());
        }
        let mut parent: Vec<Option<usize>> = vec![None; count];
        let mut stack: Vec<usize> = Vec::new();
        for (index, level) in levels.iter().enumerate() {
            while let Some(&top) = stack.last() {
                if levels[top] < *level {
                    break;
                }
                stack.pop();
            }
            parent[index] = stack.last().copied();
            stack.push(index);
        }

        let mut subtree = vec![1i64; count];
        for index in (0..count).rev() {
            if let Some(above) = parent[index] {
                subtree[above] += subtree[index];
            }
        }

        let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut root_children: Vec<usize> = Vec::new();
        for (index, above) in parent.iter().enumerate() {
            match above {
                Some(above) => children[*above].push(index),
                None => root_children.push(index),
            }
        }

        for index in 0..count {
            let parent_obj = Object::Reference(match parent[index] {
                Some(above) => item_ids[above],
                None => root_id,
            });
            let dict = self
                .inner_mut()
                .get_object_mut(item_ids[index])
                .map_err(|e| Error::Pdf(format!("signet {index} : {e}")))?
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("signet {index} : {e}")))?;
            dict.set("Parent", parent_obj);
            if let (Some(first), Some(last)) = (children[index].first(), children[index].last()) {
                dict.set("First", Object::Reference(item_ids[*first]));
                dict.set("Last", Object::Reference(item_ids[*last]));
                dict.set("Count", Object::Integer(subtree[index] - 1));
            }
        }

        self.link_siblings(&root_children, item_ids)?;
        for siblings in &children {
            self.link_siblings(siblings, item_ids)?;
        }

        let root = self
            .inner_mut()
            .get_object_mut(root_id)
            .map_err(|e| Error::Pdf(format!("racine du plan : {e}")))?
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("racine du plan : {e}")))?;
        if let (Some(first), Some(last)) = (root_children.first(), root_children.last()) {
            root.set("First", Object::Reference(item_ids[*first]));
            root.set("Last", Object::Reference(item_ids[*last]));
        }
        root.set("Count", Object::Integer(count as i64));

        Ok(())
    }

    pub(crate) fn link_siblings(&mut self, siblings: &[usize], item_ids: &[ObjectId]) -> Result<()> {
        for pair in siblings.windows(2) {
            let (before, after) = (item_ids[pair[0]], item_ids[pair[1]]);
            self.set_dict_key(before, "Next", Object::Reference(after))?;
            self.set_dict_key(after, "Prev", Object::Reference(before))?;
        }
        Ok(())
    }

    pub(crate) fn set_dict_key(&mut self, id: ObjectId, key: &str, value: Object) -> Result<()> {
        let dict = self
            .inner_mut()
            .get_object_mut(id)
            .map_err(|e| Error::Pdf(format!("signet : {e}")))?
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("signet : {e}")))?;
        dict.set(key, value);
        Ok(())
    }

    pub(crate) fn drop_outline_key(&mut self) {
        if let Ok(catalog) = self.inner_mut().catalog_mut() {
            catalog.remove(b"Outlines");
        }
    }
}