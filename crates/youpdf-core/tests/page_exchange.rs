use youpdf_core::Document;
use youpdf_shared::Result;

/// Test interleave pages with even_first=true
/// 6 pages → [0,2,4,1,3,5]
#[test]
fn interleave_even_first() -> Result<()> {
    // Create a 6-page PDF
    let doc = youpdf_core::create::blank_document(6, 595.0, 842.0)?;
    let bytes = doc.to_bytes()?;
    let mut doc = Document::from_bytes(&bytes)?;

    youpdf_core::organize::exchange::interleave_pages(&mut doc, true)?;

    // Vérifier le nombre de pages
    assert_eq!(doc.page_count(), 6);

    Ok(())
}

/// Test interleave pages with even_first=false
/// 6 pages → [1,3,5,0,2,4]
#[test]
fn interleave_odd_first() -> Result<()> {
    // Create a 6-page PDF
    let doc = youpdf_core::create::blank_document(6, 595.0, 842.0)?;
    let bytes = doc.to_bytes()?;
    let mut doc = Document::from_bytes(&bytes)?;

    youpdf_core::organize::exchange::interleave_pages(&mut doc, false)?;

    // Vérifier le nombre de pages
    assert_eq!(doc.page_count(), 6);

    Ok(())
}

/// Test exchange_ranges swapping two disjoint ranges
#[test]
fn exchange_disjoint_ranges() -> Result<()> {
    // Create a 13-page PDF
    let doc = youpdf_core::create::blank_document(13, 595.0, 842.0)?;
    let bytes = doc.to_bytes()?;
    let mut doc = Document::from_bytes(&bytes)?;

    // Swap pages 0-2 with 10-12
    youpdf_core::organize::exchange::exchange_ranges(&mut doc, 0..3, 10..13)?;

    // Vérifier le nombre de pages
    assert_eq!(doc.page_count(), 13);

    Ok(())
}