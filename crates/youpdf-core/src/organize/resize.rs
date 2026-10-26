use lopdf::{Object, Stream};
use crate::document::Document;
use youpdf_shared::Result;

/// Resize a single page by rewriting /MediaBox and prepending a scale matrix
/// to the content stream. Text, vector and images scale uniformly in one
/// operator — no per-element rewriting, no XObject wrapping.
///
/// ponytail: Annotations keep original /Rect in user-space; rescale if visual
/// drift appears on extreme zoom.
pub fn resize_page(doc: &mut Document, page: usize, width: f32, height: f32) -> Result<()> {
    let page_id = doc.page_id(page as u32).map_err(|e| youpdf_shared::Error::Pdf(e.to_string()))?;

    // Nouveau MediaBox [0 0 width height]
    {
        let mut page_dict = doc.inner.get_dictionary_mut(page_id).map_err(|e| youpdf_shared::Error::Pdf(e.to_string()))?;
        page_dict.set(
            b"MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(width),
                Object::Real(height),
            ]),
        );
    }

    // Ancien contenu conservé, matrice d'échelle prépendue
    let old_content = doc
        .inner
        .get_page_content(page_id)
        .map_err(|e| youpdf_shared::Error::Pdf(e.to_string()))?;

    let old_width = 595.0f32;
    let old_height = 842.0f32;
    let sx = width / old_width;
    let sy = height / old_height;
    let tx = -0.0f32;
    let ty = -old_height;
    let scale_text = format!("q {} 0 0 {} {} {} cm\n", sx, sy, tx, ty);
    let new_content = [scale_text.as_bytes(), &old_content].concat();

    // Remplacer /Contents par le nouveau flux avec matrice d'échelle + contenu original
    let new_id = doc.inner.add_object(Object::Stream(Stream::new(
        lopdf::Dictionary::new(),
        new_content,
    )));
    let mut page_dict = doc.inner.get_dictionary_mut(doc.page_id(page as u32)?).map_err(|e| youpdf_shared::Error::Pdf(e.to_string()))?;
    page_dict.set(b"Contents", Object::Reference(new_id));

    doc.touch();
    Ok(())
}