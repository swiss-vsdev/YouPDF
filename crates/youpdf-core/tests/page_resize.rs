use youpdf_core::Document;
use youpdf_core::organize::resize;
use youpdf_shared::Result;

/// Create a single A4 page PDF in memory.
fn a4_pdf() -> Result<Vec<u8>> {
    let doc = youpdf_core::create::blank_document(1, 595.0, 842.0)?;
    Ok(doc.to_bytes()?)
}

/// Create a single Legal page PDF in memory.
fn legal_pdf() -> Result<Vec<u8>> {
    let doc = youpdf_core::create::blank_document(1, 612.0, 612.0)?;
    Ok(doc.to_bytes()?)
}

#[test]
fn resize_a4_to_letter() -> Result<()> {
    // Create a minimal PDF with one A4 page (595 × 842)
    let bytes = a4_pdf()?;
    let mut doc = Document::from_bytes(&bytes)?;

    // Resize to Letter (612 × 792)
    resize::resize_page(&mut doc, 1, 612.0, 792.0)?;

    // Assert /MediaBox changed
    let media_box = doc
        .page_boxes(1)
        .media
        .expect("MediaBox should exist");

    assert_eq!(
        media_box,
        [0.0, 0.0, 612.0, 792.0],
        "MediaBox should be [0 0 612 792]"
    );

    Ok(())
}

#[test]
fn resize_a4_to_legal() -> Result<()> {
    // Create a minimal PDF with one A4 page (595 × 842)
    let bytes = a4_pdf()?;
    let mut doc = Document::from_bytes(&bytes)?;

    // Resize to Legal (612 × 612)
    resize::resize_page(&mut doc, 1, 612.0, 612.0)?;

    // Assert /MediaBox changed
    let media_box = doc.page_boxes(1).media.expect("MediaBox should exist");

    assert_eq!(
        media_box,
        [0.0, 0.0, 612.0, 612.0],
        "MediaBox should be [0 0 612 612]"
    );

    Ok(())
}

/// L'échelle doit venir de la boîte réelle de la page, pas d'une constante A4.
#[test]
fn resize_scales_from_actual_box() -> Result<()> {
    // Page carrée 612 × 612 : une constante A4 produirait une échelle fausse.
    let bytes = legal_pdf()?;
    let mut doc = Document::from_bytes(&bytes)?;
    resize::resize_page(&mut doc, 1, 1224.0, 1224.0)?;

    let page_id = doc.page_id(1)?;
    let content = doc.inner().get_page_content(page_id).expect("contenu de page");
    let head = String::from_utf8_lossy(&content);
    assert!(
        head.starts_with("q 2 0 0 2 0 0 cm"),
        "la matrice doit mettre la page carrée à l'échelle ×2, obtenu : {head}"
    );

    Ok(())
}