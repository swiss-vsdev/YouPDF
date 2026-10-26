use lopdf::{Object, Stream};
use crate::document::Document;
use youpdf_shared::{Error, Result};

/// Redimensionne une page en réécrivant `/MediaBox` et en prépendant une
/// matrice d'échelle au flux de contenu. Texte, vectoriel et images sont mis à
/// l'échelle par un seul opérateur — pas de réécriture élément par élément.
///
/// ponytail: les annotations gardent leur `/Rect` d'origine ; rééchelonner si
/// une dérive visuelle apparaît. La page doit avoir une `/MediaBox` lisible.
pub fn resize_page(doc: &mut Document, page: usize, width: f32, height: f32) -> Result<()> {
    let page_id = doc.page_id(page as u32)?;
    let [x0, y0, x1, y1] = doc
        .page_boxes(page as u32)
        .media
        .ok_or_else(|| Error::InvalidInput("page sans /MediaBox".into()))?;
    let (old_width, old_height) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));

    doc.inner
        .get_dictionary_mut(page_id)
        .map_err(|e| Error::Pdf(e.to_string()))?
        .set(
            b"MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(width),
                Object::Real(height),
            ]),
        );

    // Le contenu d'origine est conservé, précédé de l'échelle qui ramène
    // l'ancienne boîte dans la nouvelle (l'origine de la boîte est annulée).
    let old_content = doc
        .inner
        .get_page_content(page_id)
        .map_err(|e| Error::Pdf(e.to_string()))?;
    let sx = width / old_width;
    let sy = height / old_height;
    // `+ 0.0` normalise -0 en 0 dans la matrice imprimée.
    let scale = format!("q {sx} 0 0 {sy} {} {} cm\n", -x0 * sx + 0.0, -y0 * sy + 0.0);
    let new_id = doc.inner.add_object(Object::Stream(Stream::new(
        lopdf::Dictionary::new(),
        [scale.as_bytes(), &old_content].concat(),
    )));
    doc.inner
        .get_dictionary_mut(page_id)
        .map_err(|e| Error::Pdf(e.to_string()))?
        .set(b"Contents", Object::Reference(new_id));

    doc.touch();
    Ok(())
}
