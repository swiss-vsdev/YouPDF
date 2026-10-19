//! Détection d'une page scannée : image pleine page, aucun texte.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::Document;

fn pdf_avec_image() -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut image = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => 8,
        },
        vec![255, 255, 255],
    );
    image.dict.set("Length", 3);
    let image_id = doc.add_object(image);
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 612 0 0 792 0 0 cm /Im0 Do Q".to_vec(),
    ));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => pages_id,
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Im0" => image_id },
        },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![page.into()]),
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");
    bytes
}

#[test]
fn page_pleine_image_est_un_scan() {
    let doc = Document::from_bytes(&pdf_avec_image()).expect("ouverture");
    assert!(
        (doc.page_image_coverage(1) - 1.0).abs() < 0.01,
        "couverture {}",
        doc.page_image_coverage(1)
    );
    assert!(doc.looks_scanned(1), "page image sans texte = scan");
}

#[test]
fn page_avec_texte_n_est_pas_un_scan() {
    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 50 700 Td (Bonjour) Tj ET".to_vec(),
    ));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => pages_id,
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![page.into()]),
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");

    let doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(!doc.looks_scanned(1), "du texte extractible exclut le scan");
}
