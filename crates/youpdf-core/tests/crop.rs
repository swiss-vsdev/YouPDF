//! Test de fumée du recadrage (`/CropBox`).
//!
//! Chaque test échoue si le calcul des marges, la préservation de l'origine,
//! la lecture du `/CropBox` propre ou la remise à zéro cassent.

use lopdf::{dictionary, Document as LopdfDoc, Object};
use youpdf_core::{CropMargins, Document};

/// Construit un PDF d'une page par boîte `/MediaBox` fournie.
fn pdf_with_boxes(boxes: &[[i64; 4]]) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for b in boxes {
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                b[0].into(), b[1].into(), b[2].into(), b[3].into(),
            ]),
        });
        kids.push(page_id.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => boxes.len() as i64,
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");
    buf
}

/// Un PDF A4 d'une page.
fn a4() -> Vec<u8> {
    pdf_with_boxes(&[[0, 0, 595, 842]])
}

#[test]
fn crop_removes_margins_on_each_side() {
    let mut doc = Document::from_bytes(&a4()).expect("ouverture");
    let changed = doc
        .crop_pages(&[1], CropMargins::default())
        .expect("recadrage");
    assert_eq!(changed, 1, "une page recadrée");

    let view = doc.page_view(1);
    assert!((view.width - 559.0).abs() < 0.01, "largeur 595 - 2*18");
    assert!((view.height - 806.0).abs() < 0.01, "hauteur 842 - 2*18");
}

#[test]
fn crop_keeps_box_origin() {
    // Boîte qui ne part pas de (0, 0).
    let mut doc = Document::from_bytes(&pdf_with_boxes(&[[50, 60, 595, 842]])).expect("ouverture");
    let before = doc.page_view(1);
    assert!((before.x - 50.0).abs() < 0.01);
    assert!((before.y - 60.0).abs() < 0.01);

    doc.crop_pages(&[1], CropMargins::default()).expect("recadrage");

    let view = doc.page_view(1);
    assert!((view.x - 68.0).abs() < 0.01, "origine x = 50 + 18");
    assert!((view.y - 78.0).abs() < 0.01, "origine y = 60 + 18");
    assert!((view.width - (595.0 - 50.0 - 36.0)).abs() < 0.01);
    assert!((view.height - (842.0 - 60.0 - 36.0)).abs() < 0.01);
}

#[test]
fn crop_box_returns_set_box() {
    let mut doc = Document::from_bytes(&a4()).expect("ouverture");
    assert!(doc.crop_box(1).is_none(), "aucun /CropBox au départ");

    doc.crop_pages(&[1], CropMargins::default()).expect("recadrage");

    let box_ = doc.crop_box(1).expect("/CropBox posé");
    for (got, want) in box_.iter().zip([18.0, 18.0, 577.0, 824.0]) {
        assert!((got - want).abs() < 0.01, "boîte {box_:?}");
    }
}

#[test]
fn reset_crop_restores_original_size() {
    let mut doc = Document::from_bytes(&a4()).expect("ouverture");
    doc.crop_pages(&[1], CropMargins::default()).expect("recadrage");
    assert!((doc.page_view(1).width - 559.0).abs() < 0.01);

    let changed = doc.reset_crop(&[1]).expect("remise à zéro");
    assert_eq!(changed, 1);

    let view = doc.page_view(1);
    assert!((view.width - 595.0).abs() < 0.01, "taille d'origine restaurée");
    assert!((view.height - 842.0).abs() < 0.01);
    assert!((view.x).abs() < 0.01);
    assert!((view.y).abs() < 0.01);
}

#[test]
fn over_large_margin_clamps_to_one_point() {
    let mut doc = Document::from_bytes(&a4()).expect("ouverture");
    let huge = CropMargins {
        left: 1000.0,
        bottom: 1000.0,
        right: 1000.0,
        top: 1000.0,
    };
    doc.crop_pages(&[1], huge).expect("recadrage");

    let view = doc.page_view(1);
    assert!((view.width - 1.0).abs() < 0.01, "largeur ramenée à 1 pt");
    assert!((view.height - 1.0).abs() < 0.01, "hauteur ramenée à 1 pt");
}
