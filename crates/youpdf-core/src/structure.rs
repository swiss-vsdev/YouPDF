//! Structure du document : signets (plan) et pièces jointes.
//!
//! Lecture seule : ces deux structures viennent du catalogue PDF et ne sont
//! pas modifiées par l'application.

use lopdf::{Dictionary, Object, ObjectId};

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

    /// Reconstruit le plan à partir du texte.
    ///
    /// Une ligne dont la taille dépasse nettement celle du corps du texte
    /// devient un signet ; les tailles distinctes sont rangées en niveaux,
    /// de la plus grande à la plus petite. L'ancien plan est remplacé.
    pub fn generate_outline(&mut self, style: &OutlineStyle) -> Result<usize> {
        let headings = self.detect_headings(style);
        // Toute arborescence précédente est retirée avant d'écrire la neuve.
        self.drop_outline_key();

        if headings.is_empty() {
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

        self.touch();
        Ok(headings.len())
    }

    /// Supprime tous les signets (`/Outlines` du catalogue).
    ///
    /// Renvoie le nombre d'entrées qui existaient avant la suppression.
    pub fn clear_outline(&mut self) -> Result<usize> {
        let count = self.outline().len();
        self.drop_outline_key();
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
        let count = headings.len();
        // Parent de chaque entrée : le dernier titre de niveau supérieur.
        let mut parent: Vec<Option<usize>> = vec![None; count];
        let mut stack: Vec<usize> = Vec::new();
        for (index, heading) in headings.iter().enumerate() {
            while let Some(&top) = stack.last() {
                if headings[top].level < heading.level {
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
        // Un `/Kids` cyclique re-listerait les mêmes pièces jointes jusqu'à
        // la limite de garde : chaque nœud n'est visité qu'une fois.
        let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        let mut listed: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        while let Some(node) = nodes.pop() {
            if let Ok(entries) = node.get(b"Names").and_then(Object::as_array) {
                // Un tableau impair (fichier abîmé ou hostile) n'a pas de
                // seconde moitié : la paire est simplement ignorée.
                for pair in entries.chunks(2) {
                    let Some(spec_id) = pair.get(1).and_then(|obj| self.resolve_id(obj)) else {
                        continue;
                    };
                    // Le même nœud atteint par deux chemins ne liste pas
                    // deux fois la même pièce jointe.
                    if !listed.insert(spec_id) {
                        continue;
                    }
                    let Ok(spec) = self.inner.get_dictionary(spec_id) else {
                        continue;
                    };
                    if let Some(info) = self.describe_attachment(spec) {
                        files.push(info);
                    }
                }
            }
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
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
        // Un `/Kids` cyclique ferait tourner la boucle sans fin : chaque
        // nœud n'est visité qu'une fois.
        let mut seen: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        while let Some(node) = nodes.pop() {
            if let Ok(entries) = node.get(b"Names").and_then(Object::as_array) {
                for pair in entries.chunks(2) {
                    let Some(spec) = pair.get(1).and_then(|obj| self.dict_object(obj)) else {
                        continue;
                    };
                    if let Some(id) = self
                        .dict_entry(&spec, b"EF")
                        .and_then(|ef| ef.get(b"UF").or_else(|_| ef.get(b"F")).ok())
                        .and_then(|obj| self.resolve_id(obj))
                    {
                        if !streams.contains(&id) {
                            streams.push(id);
                        }
                    }
                }
            }
            if let Ok(kids) = node.get(b"Kids").and_then(Object::as_array) {
                for kid in kids {
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
