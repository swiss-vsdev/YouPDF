use youpdf_core::Document;
use youpdf_shared::Result;

/// Ordre initial des pages, du premier au dernier.
fn order(doc: &Document) -> Vec<lopdf::ObjectId> {
    doc.page_ids().into_values().collect()
}

/// Reconstruit un document depuis ses octets (l'arbre des pages est figé).
fn reload(doc: &Document) -> Result<Document> {
    Document::from_bytes(&doc.to_bytes()?)
}

/// 6 pages → [0,2,4,1,3,5]
#[test]
fn interleave_even_first() -> Result<()> {
    let doc = youpdf_core::create::blank_document(6, 595.0, 842.0)?;
    let ids = order(&doc);
    let mut doc = reload(&doc)?;

    youpdf_core::organize::exchange::interleave_pages(&mut doc, true)?;

    assert_eq!(order(&doc), vec![ids[0], ids[2], ids[4], ids[1], ids[3], ids[5]]);
    // L'ordre doit survivre à une sauvegarde : l'arbre est réellement réécrit.
    assert_eq!(order(&reload(&doc)?), vec![ids[0], ids[2], ids[4], ids[1], ids[3], ids[5]]);
    Ok(())
}

/// 6 pages → [1,3,5,0,2,4]
#[test]
fn interleave_odd_first() -> Result<()> {
    let doc = youpdf_core::create::blank_document(6, 595.0, 842.0)?;
    let ids = order(&doc);
    let mut doc = reload(&doc)?;

    youpdf_core::organize::exchange::interleave_pages(&mut doc, false)?;

    assert_eq!(order(&doc), vec![ids[1], ids[3], ids[5], ids[0], ids[2], ids[4]]);
    Ok(())
}

/// Échange les plages 0..3 et 10..13 d'un document de 13 pages.
#[test]
fn exchange_disjoint_ranges() -> Result<()> {
    let doc = youpdf_core::create::blank_document(13, 595.0, 842.0)?;
    let ids = order(&doc);
    let mut doc = reload(&doc)?;

    youpdf_core::organize::exchange::exchange_ranges(&mut doc, 0..3, 10..13)?;

    let expected = [10, 11, 12, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2]
        .iter()
        .map(|&i| ids[i])
        .collect::<Vec<_>>();
    assert_eq!(order(&doc), expected);
    assert_eq!(order(&reload(&doc)?), expected);
    Ok(())
}

/// Des plages qui se chevauchent sont refusées, pas appliquées à moitié.
#[test]
fn exchange_overlapping_ranges_rejected() -> Result<()> {
    let doc = youpdf_core::create::blank_document(6, 595.0, 842.0)?;
    let mut doc = reload(&doc)?;

    assert!(youpdf_core::organize::exchange::exchange_ranges(&mut doc, 2..5, 4..6).is_err());
    Ok(())
}
