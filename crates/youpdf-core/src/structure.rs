//! Structure du document : signets (plan) et pièces jointes.
//!
//! Lecture, écriture du plan (le `/Outlines` du catalogue) et lecture des
//! pièces jointes.

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::{encode_pdf_literal, TextFragment};

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

/// Part du document transformée en signets.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineStyle {
    /// Profondeur maximale conservée (1 = seulement la plus grande taille).
    pub max_levels: usize,
    /// Ignore les titres plus petits que cette taille, en points.
    pub min_font_size: f32,
    /// Nombre maximal de signets au total (0 = aucune limite).
    pub max_entries: usize,
}

impl Default for OutlineStyle {
    fn default() -> Self {
        OutlineStyle {
            max_levels: 3,
            min_font_size: 10.0,
            max_entries: 500,
        }
    }
}

/// Titre candidat, avant écriture de l'arbre `/Outlines`.
#[derive(Debug, Clone)]
struct DetectedHeading {
    title: String,
    page: u32,
    left: f32,
    top: f32,
    level: usize,
}

/// Ligne reconstituée à partir de fragments de texte.
struct TextLine {
    page: u32,
    baseline: f32,
    font_size: f32,
    left: f32,
    top: f32,
    parts: Vec<(f32, String)>,
}

impl TextLine {
    fn new(page: u32, fragment: &TextFragment) -> Self {
        TextLine {
            page,
            baseline: fragment.rect[1],
            font_size: fragment.font_size,
            left: fragment.rect[0],
            top: fragment.rect[3],
            parts: vec![(fragment.rect[0], fragment.text.clone())],
        }
    }

    fn push(&mut self, fragment: &TextFragment) {
        self.font_size = self.font_size.max(fragment.font_size);
        self.left = self.left.min(fragment.rect[0]);
        self.top = self.top.max(fragment.rect[3]);
        self.parts.push((fragment.rect[0], fragment.text.clone()));
    }

    fn text(&self) -> String {
        let mut parts = self.parts.clone();
        parts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        parts
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_owned()
    }
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
        // Les objets de l'ancien plan sont libérés après réécriture, sauf s'ils
        // sont encore visés ailleurs.
        let doomed = self.outline_object_ids();
        self.drop_outline_key();

        if entries.is_empty() {
            self.purge_unreachable(&doomed);
            self.touch();
            return Ok(0);
        }

        // Aplatir l'arbre : un niveau par profondeur, l'ordre d'affichage
        // donnant l'ordre des frères.
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
                // Cible d'origine reprise telle quelle : la position de vue et
                // les actions traversent un renommage ou un déplacement. Une
                // destination qui vise une page supprimée depuis est abandonnée
                // plutôt que réécrite avec une référence morte.
                OutlineTarget::Dest(object) => {
                    // Un tableau dont le premier élément est une référence de
                    // page morte est abandonné. Une destination nommée, une
                    // action indirecte ou un tableau référencé gardent leur
                    // forme : rien n'est plus risqué que de les réécrire.
                    let alive = match object {
                        Object::Array(items) => match items.first() {
                            Some(Object::Reference(id)) => pages.values().any(|page| page == id),
                            _ => true,
                        },
                        _ => true,
                    };
                    // `/Dest null` n'est pas une destination valide : une cible
                    // morte se traduit par l'absence de clé.
                    if alive {
                        dict.set("Dest", object.clone());
                    }
                }
                OutlineTarget::Action(object) => {
                    // Une clé `/Dest` masquerait `/A` à la relecture.
                    dict.set("A", object.clone());
                }
                OutlineTarget::None => {
                    // Sans page connue, pas de destination du tout.
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

    /// Premier signet du plan, s'il existe.
    fn outline_first(&self) -> Option<ObjectId> {
        let catalog = self.inner.catalog().ok()?;
        let outlines = self.dict_entry(catalog, b"Outlines")?;
        outlines.get(b"First").ok().and_then(|o| self.resolve_id(o))
    }

    /// Frères d'un signet, chacun avec ses propres sous-entrées.
    ///
    /// `depth` borne la descente (un `/First` qui se prend lui-même en boucle
    /// ferait déborder la pile) et `budget` borne le nombre total d'entrées.
    fn outline_siblings(
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

    /// Cible écrite d'une entrée de plan (`/Dest` puis `/A`), telle quelle.
    fn outline_target(&self, node: &Dictionary) -> OutlineTarget {
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

    /// Objets de l'ancien plan : la racine `/Outlines` et chaque signet.
    fn outline_object_ids(&self) -> Vec<ObjectId> {
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

    /// Retire du magasin les objets devenus inatteignables.
    ///
    /// Le balayage part des racines du trailer : ce qui a été remplacé n'y
    /// est plus. Un objet que des références vivantes partagent avec l'ancien
    /// (destination nommée, lien vers un signet, pièce jointe réutilisée)
    /// reste donc atteint et survit.
    fn purge_unreachable(&mut self, doomed: &[ObjectId]) {
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

    /// Reconstruit le plan à partir du texte.
    ///
    /// Une ligne dont la taille dépasse nettement celle du corps du texte
    /// devient un signet ; les tailles distinctes sont rangées en niveaux,
    /// de la plus grande à la plus petite. L'ancien plan est remplacé.
    pub fn generate_outline(&mut self, style: &OutlineStyle) -> Result<usize> {
        let headings = self.detect_headings(style);
        // Toute arborescence précédente est retirée avant d'écrire la neuve.
        let doomed = self.outline_object_ids();
        self.drop_outline_key();

        if headings.is_empty() {
            self.purge_unreachable(&doomed);
            self.touch();
            return Ok(0);
        }

        let pages = self.inner.get_pages();
        let mut item_ids: Vec<ObjectId> = Vec::with_capacity(headings.len());
        for heading in &headings {
            // La destination reste dans la boîte visible de la page.
            let view = self.page_view(heading.page);
            let left = heading.left.max(view.x);
            let top = heading.top.min(view.y + view.height);
            let dest = match pages.get(&heading.page).copied() {
                Some(page_id) => Object::Array(vec![
                    Object::Reference(page_id),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Real(left),
                    Object::Real(top),
                    Object::Null,
                ]),
                None => Object::Null,
            };
            let dict = Dictionary::from_iter(vec![
                (
                    "Title",
                    Object::string_literal(encode_pdf_literal(&heading.title)),
                ),
                ("Dest", dest),
            ]);
            let id = self.inner_mut().add_object(Object::Dictionary(dict));
            item_ids.push(id);
        }

        let root_id = self
            .inner_mut()
            .add_object(Object::Dictionary(Dictionary::from_iter(vec![(
                "Type",
                Object::Name(b"Outlines".to_vec()),
            )])));

        self.link_outline_tree(&headings, &item_ids, root_id)?;

        self.inner_mut()
            .catalog_mut()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .set("Outlines", Object::Reference(root_id));

        self.purge_unreachable(&doomed);
        self.touch();
        Ok(headings.len())
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

    /// Détecte les titres du document, dans l'ordre page puis haut-bas.
    fn detect_headings(&self, style: &OutlineStyle) -> Vec<DetectedHeading> {
        let mut lines: Vec<TextLine> = Vec::new();
        let mut sizes: Vec<u32> = Vec::new();
        for page in 1..=self.page_count() as u32 {
            let Ok(fragments) = self.extract_text_fragments(page) else {
                continue;
            };
            for fragment in &fragments {
                sizes.push(size_key(fragment.font_size));
            }
            lines.extend(group_lines(page, &fragments));
        }

        // Taille du corps : la plus fréquente dans tout le document.
        let Some(body) = most_frequent(&sizes) else {
            return Vec::new();
        };
        let body = body as f32 / 10.0;

        let candidates: Vec<(f32, DetectedHeading)> = lines
            .into_iter()
            .filter(|line| line.font_size >= body * 1.15 && line.font_size >= style.min_font_size)
            .map(|line| {
                (
                    line.font_size,
                    DetectedHeading {
                        title: line.text(),
                        page: line.page,
                        left: line.left,
                        top: line.top,
                        level: 0,
                    },
                )
            })
            .collect();

        // Bandes de tailles distinctes, de la plus grande à la plus petite.
        let bands: Vec<u32> = candidates
            .iter()
            .map(|(size, _)| size_key(*size))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .rev()
            .collect();

        let mut headings: Vec<DetectedHeading> = candidates
            .into_iter()
            .filter_map(|(size, mut heading)| {
                let level = bands.iter().position(|band| *band == size_key(size))?;
                if level >= style.max_levels {
                    return None;
                }
                heading.level = level;
                Some(heading)
            })
            .collect();

        if style.max_entries > 0 && headings.len() > style.max_entries {
            headings.truncate(style.max_entries);
        }
        headings
    }

    /// Relie les entrées entre elles et à la racine du plan.
    fn link_outline_tree(
        &mut self,
        headings: &[DetectedHeading],
        item_ids: &[ObjectId],
        root_id: ObjectId,
    ) -> Result<()> {
        let levels: Vec<usize> = headings.iter().map(|heading| heading.level).collect();
        self.link_outline_levels(&levels, item_ids, root_id)
    }

    /// Relie une suite d'entrées déjà créées, à partir de leurs niveaux.
    ///
    /// Le parent d'une entrée est la dernière entrée rencontrée dont le niveau
    /// est plus faible : c'est la règle qui donne l'arbre des niveaux.
    fn link_outline_levels(
        &mut self,
        levels: &[usize],
        item_ids: &[ObjectId],
        root_id: ObjectId,
    ) -> Result<()> {
        let count = levels.len();
        if count == 0 || item_ids.len() != count {
            return Ok(());
        }
        // Parent de chaque entrée : le dernier niveau supérieur rencontré.
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

        // Taille du sous-arbre (entrée incluse), pour `/Count`.
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

    /// Chaîne `/Prev` `/Next` entre entrées sœurs.
    fn link_siblings(&mut self, siblings: &[usize], item_ids: &[ObjectId]) -> Result<()> {
        for pair in siblings.windows(2) {
            let (before, after) = (item_ids[pair[0]], item_ids[pair[1]]);
            self.set_dict_key(before, "Next", Object::Reference(after))?;
            self.set_dict_key(after, "Prev", Object::Reference(before))?;
        }
        Ok(())
    }

    /// Écrit une clé dans le dictionnaire d'un objet.
    fn set_dict_key(&mut self, id: ObjectId, key: &str, value: Object) -> Result<()> {
        let dict = self
            .inner_mut()
            .get_object_mut(id)
            .map_err(|e| Error::Pdf(format!("signet : {e}")))?
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("signet : {e}")))?;
        dict.set(key, value);
        Ok(())
    }

    /// Retire la clé `/Outlines` du catalogue, si présente.
    fn drop_outline_key(&mut self) {
        if let Ok(catalog) = self.inner_mut().catalog_mut() {
            catalog.remove(b"Outlines");
        }
    }

    /// Pièces jointes déclarées dans `/Names /EmbeddedFiles`.
    pub fn attachments(&self) -> Vec<AttachmentInfo> {
        self.embedded_file_entries()
            .iter()
            .filter_map(|(name, value)| {
                let mut info = self.describe_attachment(&self.dict_object(value)?)?;
                // Le nom vient de la marche : lui seul connaît la clé de
                // l'arbre quand le descripteur n'en porte aucun.
                info.name = name.clone();
                Some(info)
            })
            .collect()
    }

    /// Entrées de l'arbre `/EmbeddedFiles`, dans l'ordre du document.
    ///
    /// Chaque entrée rend son nom affiché et sa valeur d'origine (référence
    /// ou dictionnaire en ligne), pour qu'une réécriture de l'arbre ne perde
    /// rien. Une entrée n'est retenue que si sa valeur se résout en
    /// dictionnaire de descripteur : la liste, l'extraction et le retrait
    /// indexent donc exactement le même ensemble.
    fn embedded_file_entries(&self) -> Vec<(String, Object)> {
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
        // Un `/Kids` cyclique ferait tourner la boucle sans fin : chaque nœud
        // n'est visité qu'une fois. Le nœud racine compte comme visité, sinon
        // un arbre qui se référence lui-même listerait deux fois ses entrées.
        let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        if let Ok(root_id) = names.get(b"EmbeddedFiles").and_then(Object::as_reference) {
            seen.insert(root_id);
        }
        while let Some(node) = nodes.pop() {
            if let Ok(pairs) = node.get(b"Names").and_then(Object::as_array) {
                // Un tableau impair (fichier abîmé ou hostile) n'a pas de
                // seconde moitié : la paire est simplement ignorée.
                for pair in pairs.chunks(2) {
                    let (Some(key), Some(value)) = (pair.first(), pair.get(1)) else {
                        continue;
                    };
                    let Some(filespec) = self.dict_object(value) else {
                        continue;
                    };
                    // Le nom affiché vient du descripteur (`/UF` puis `/F`) ;
                    // la clé de l'arbre prend le relais quand il n'en porte
                    // aucun, sinon deux descripteurs anonymes se
                    // confondraient.
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
                // Empilés à l'envers : la pile les ressort dans l'ordre du
                // document, comme les autres lectures.
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

    /// Objets d'une entrée de pièce jointe : le descripteur et ses flux,
    /// quand ils sont indirects.
    fn attachment_object_ids(&self, value: &Object) -> Vec<ObjectId> {
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
        // Certains producteurs omettent `/Filter` ou `/Length` : dans ce cas
        // le contenu brut est déjà le fichier.
        Ok(stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()))
    }

    /// Ajoute une pièce jointe embarquée, ou remplace celle du même nom.
    ///
    /// Le fichier devient un `/Filespec` référencé par l'arbre
    /// `/Names /EmbeddedFiles` du catalogue, comme pour une pièce jointe
    /// créée par Acrobat. L'arbre est réécrit à plat : un seul nœud porte
    /// toutes les entrées, les anciennes reprises telles quelles.
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

        // Flux du fichier, puis son descripteur.
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

        // Entrées existantes, celles du même nom remplacées. Un nom ne
        // désigne qu'un fichier : des doublons du même nom partent tous, et
        // le nouveau descripteur seul reste.
        let mut entries: Vec<(String, Object)> = Vec::new();
        let mut doomed = self.embedded_tree_ids();
        let mut replaced = false;
        for (existing, value) in self.embedded_file_entries() {
            if existing == file_name {
                // Les descripteurs remplacés et leurs flux ne sont plus
                // atteints : ils partent avec l'ancien arbre.
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
    ///
    /// Le descripteur et son flux disparaissent s'ils ne sont plus atteints
    /// depuis le catalogue.
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

        // L'ancien arbre, le descripteur et ses flux perdent leur dernier
        // chemin d'accès ; ceux que d'autres références visent encore
        // survivent au balayage.
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

    /// Identifiants des objets qui composent l'arbre `/EmbeddedFiles` : ils
    /// deviennent orphelins dès que le catalogue pointe ailleurs.
    fn embedded_tree_ids(&self) -> Vec<ObjectId> {
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
        // L'arbre racine peut être un objet indirect ou un dictionnaire en
        // ligne ; ses `/Kids`, eux, sont toujours des références.
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

    /// Écrit un arbre `/EmbeddedFiles` à un seul nœud, rattaché au catalogue.
    fn write_embedded_tree(&mut self, entries: &[(String, Object)]) -> Result<()> {
        let mut names: Vec<Object> = Vec::with_capacity(entries.len() * 2);
        for (name, filespec) in entries {
            names.push(Object::string_literal(name.as_str()));
            names.push(filespec.clone());
        }
        let leaf = self.inner_mut().add_object(Object::Dictionary(dictionary! {
            "Names" => Object::Array(names),
        }));

        // `/Names` du catalogue : une référence indirecte s'édite sur place,
        // un dictionnaire direct s'édite à travers le catalogue.
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

    /// Retire l'arbre `/EmbeddedFiles` du catalogue, faute d'entrées.
    fn drop_embedded_files(&mut self) {
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

/// Clé de comparaison d'une taille de police, arrondie au dixième.
fn size_key(size: f32) -> u32 {
    (size.max(0.0) * 10.0).round() as u32
}

/// Taille la plus fréquente (à égalité, la plus petite).
fn most_frequent(sizes: &[u32]) -> Option<u32> {
    let mut counts: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for &size in sizes {
        *counts.entry(size).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(size, _)| size)
}

/// Regroupe des fragments en lignes, de haut en bas.
fn group_lines(page: u32, fragments: &[TextFragment]) -> Vec<TextLine> {
    let mut sorted: Vec<&TextFragment> = fragments.iter().collect();
    sorted.sort_by(|a, b| {
        b.rect[1]
            .partial_cmp(&a.rect[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<TextLine> = Vec::new();
    for fragment in sorted {
        match lines.last_mut() {
            Some(line) if (line.baseline - fragment.rect[1]).abs() <= 2.0 => line.push(fragment),
            _ => lines.push(TextLine::new(page, fragment)),
        }
    }
    lines
}

/// Décode une chaîne PDF : UTF-16BE avec BOM, sinon Latin-1/ASCII.
/// Rassemble les identifiants d'objets référencés par `object`, en profondeur.
fn collect_references(object: &Object, out: &mut std::collections::HashSet<ObjectId>) {
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
