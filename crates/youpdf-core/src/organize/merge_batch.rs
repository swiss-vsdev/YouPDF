//! Batch page merging: combine multiple pages onto a single output page.
//!
//! Unlike UPDF's "merge" feature that combines multiple source documents, this
//! implements the "batch merge" operation that places multiple source pages
//! onto a single output canvas — like printing 2-up or 4-up.

use std::collections::HashSet;

use lopdf::{Dictionary, Object};
use youpdf_shared::Result;

use crate::document::Document;

/// Merge selected pages into groups, each group placed on a single output page.
///
/// `per_output` controls how many source pages go on each output page.
/// Pages are merged in order; the first `per_output` pages become page 1,
/// the next `per_output` become page 2, etc.
///
/// Each output page has dimensions `width × height`. Source pages are scaled
/// to fit in their slot and arranged in a row-major grid.
///
/// After merging, the selected pages are removed and replaced by the new
/// merged pages, which are inserted in place at the first selected position.
pub fn merge_pages(
    doc: &mut Document,
    pages: &[usize],
    per_output: usize,
    width: f32,
    height: f32,
) -> Result<usize> {
    if pages.is_empty() || per_output < 2 {
        return Ok(0);
    }

    let total = doc.page_count();
    let valid: Vec<usize> = pages
        .iter()
        .copied()
        .filter(|p| *p >= 1 && *p <= total)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .into_iter()
        .collect();
    if valid.len() < 2 {
        return Ok(0);
    }

    // Capture source page data before mutation.
    let mut source_pages = Vec::new();
    for &page in &valid {
        let page_id = doc.page_id(page as u32)?;
        let view = doc.page_view(page as u32);
        let content = doc
            .inner()
            .get_page_content(page_id)
            .map_err(|e| youpdf_shared::Error::Pdf(format!("page content: {e}")))?;
        let resources = doc
            .inner()
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Resources").ok().cloned())
            .unwrap_or(Object::Dictionary(lopdf::Dictionary::new()));
        source_pages.push((view, content, resources));
    }

    // Compute tile dimensions for the grid.
    let cols = (per_output as f32).sqrt().ceil() as usize;
    let cols = cols.max(1).min(source_pages.len());
    let rows = (source_pages.len() + cols - 1) / cols;
    let _tile_w = width / cols as f32;
    let _tile_h = height / rows as f32;

    // Create merged output pages.
    let mut new_ids = Vec::new();
    for chunk in source_pages.chunks(per_output) {
        let ncols = (chunk.len() as f32).sqrt().ceil() as usize;
        let ncols = ncols.max(1).min(chunk.len());
        let nrows = (chunk.len() + ncols - 1) / ncols;
        let tw = width / ncols as f32;
        let th = height / nrows as f32;

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(width),
                Object::Real(height),
            ]),
        );

        // Merge content streams.
        let mut content = Vec::new();
        for (i, (view, src_content, _res)) in chunk.iter().enumerate() {
            let col = (i % ncols) as f32;
            let row = (i / ncols) as f32;
            let x = col * tw;
            let y = height - (row + 1.0) * th;
            let sx = tw / view.width.max(1.0);
            let sy = th / view.height.max(1.0);
            content.extend_from_slice(
                format!("q {sx} 0 0 {sy} {} {} cm\n", x, y)
                    .as_bytes(),
            );
            content.extend_from_slice(src_content);
            content.extend_from_slice(b"\nQ");
        }
        page.set(
            "Contents",
            Object::Stream(lopdf::Stream::new(lopdf::Dictionary::new(), content)),
        );
        page.set("Resources", Object::Dictionary(lopdf::Dictionary::new()));
        new_ids.push(doc.inner.add_object(Object::Dictionary(page)));
    }

    // Remove source pages and insert merged pages at first position.
    let first = valid.iter().copied().min().unwrap_or(1);
    doc.delete_pages(&valid.iter().map(|p| p - first + 1).collect::<Vec<_>>())?;

    // Insert new pages after the first deleted position.
    let total_after = doc.page_count();
    let anchor = if first <= total_after {
        doc.inner.get_pages().get(&((first.saturating_sub(1)) as u32)).copied()
    } else {
        None
    };
    let root = doc.pages_root_id()?;
    doc.insert_kids(root, anchor, &new_ids)?;
    doc.refresh_page_tree()?;
    doc.touch();

    Ok(new_ids.len())
}
