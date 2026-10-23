use lopdf::{Dictionary, Object, ObjectId};
use youpdf_shared::{Error, Result};
use crate::document::Document;
use crate::edit::{encode_pdf_literal, TextFragment};

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
pub(crate) struct DetectedHeading {
    pub(crate) title: String,
    pub(crate) page: u32,
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) level: usize,
}

/// Ligne reconstituée à partir de fragments de texte.
pub(crate) struct TextLine {
    pub(crate) page: u32,
    pub(crate) baseline: f32,
    pub(crate) font_size: f32,
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) parts: Vec<(f32, String)>,
}

impl TextLine {
    pub(crate) fn new(page: u32, fragment: &TextFragment) -> Self {
        TextLine {
            page,
            baseline: fragment.rect[1],
            font_size: fragment.font_size,
            left: fragment.rect[0],
            top: fragment.rect[3],
            parts: vec![(fragment.rect[0], fragment.text.clone())],
        }
    }

    pub(crate) fn push(&mut self, fragment: &TextFragment) {
        self.font_size = self.font_size.max(fragment.font_size);
        self.left = self.left.min(fragment.rect[0]);
        self.top = self.top.max(fragment.rect[3]);
        self.parts.push((fragment.rect[0], fragment.text.clone()));
    }

    pub(crate) fn text(&self) -> String {
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

impl Document {
    /// Reconstruit le plan à partir du texte.
    ///
    /// Une ligne dont la taille dépasse nettement celle du corps du texte
    /// devient un signet ; les tailles distinctes sont rangées en niveaux,
    /// de la plus grande à la plus petite. L'ancien plan est remplacé.
    pub fn generate_outline(&mut self, style: &OutlineStyle) -> Result<usize> {
        let headings = self.detect_headings(style);
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

    pub(crate) fn detect_headings(&self, style: &OutlineStyle) -> Vec<DetectedHeading> {
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

    pub(crate) fn link_outline_tree(
        &mut self,
        headings: &[DetectedHeading],
        item_ids: &[ObjectId],
        root_id: ObjectId,
    ) -> Result<()> {
        let levels: Vec<usize> = headings.iter().map(|heading| heading.level).collect();
        self.link_outline_levels(&levels, item_ids, root_id)
    }
}

pub(crate) fn size_key(size: f32) -> u32 {
    (size.max(0.0) * 10.0).round() as u32
}

pub(crate) fn most_frequent(sizes: &[u32]) -> Option<u32> {
    let mut counts: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for &size in sizes {
        *counts.entry(size).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(size, _)| size)
}

pub(crate) fn group_lines(page: u32, fragments: &[TextFragment]) -> Vec<TextLine> {
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