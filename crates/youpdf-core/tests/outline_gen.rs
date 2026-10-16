//! Génération automatique du plan (signets) à partir du texte.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::structure::{decode_pdf_text, OutlineStyle};
use youpdf_core::Document;

/// Construit un PDF dont chaque page porte des lignes de texte positionnées.
///
/// Chaque ligne est `(texte, taille de police, x, y)`.
fn text_pdf(pages: &[Vec<(&str, f32, f32, f32)>]) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for lines in pages {
        let mut content: Vec<u8> = Vec::new();
        for (text, size, x, y) in lines {
            content.extend_from_slice(
                format!("BT\n/F1 {size} Tf\n1 0 0 1 {x} {y} Tm\n({text}) Tj\nET\n").as_bytes(),
            );
        }
        let stream = doc.add_object(Stream::new(dictionary! {}, content));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => dictionary! {
                        "Type" => Object::Name(b"Font".to_vec()),
                        "Subtype" => Object::Name(b"Type1".to_vec()),
                        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
                    }
                }
            },
            "Contents" => stream,
        });
        kids.push(page.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => pages.len() as i64,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture du PDF de test");
    buf
}

#[test]
fn generate_produces_expected_count_and_nesting() {
    let pdf = text_pdf(&[
        vec![
            ("Chapter One", 24.0, 72.0, 720.0),
            ("Section A", 16.0, 72.0, 690.0),
            ("body one", 11.0, 72.0, 660.0),
            ("body two", 11.0, 72.0, 640.0),
        ],
        vec![
            ("Chapter Two", 24.0, 72.0, 720.0),
            ("Section B", 16.0, 72.0, 690.0),
            ("body three", 11.0, 72.0, 660.0),
            ("body four", 11.0, 72.0, 640.0),
        ],
    ]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let written = doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(written, 4, "deux chapitres et deux sections");

    let outline = doc.outline();
    assert_eq!(outline.len(), 4);
    let find = |title: &str| outline.iter().find(|item| item.title == title);
    assert_eq!(find("Chapter One").map(|item| item.level), Some(0));
    assert_eq!(find("Chapter Two").map(|item| item.level), Some(0));
    assert_eq!(find("Section A").map(|item| item.level), Some(1));
    assert_eq!(find("Section B").map(|item| item.level), Some(1));
    assert_eq!(find("Chapter One").and_then(|item| item.page), Some(1));
    assert_eq!(find("Chapter Two").and_then(|item| item.page), Some(2));
}

#[test]
fn generated_top_level_order_follows_the_document() {
    let pdf = text_pdf(&[
        vec![("First", 24.0, 72.0, 720.0), ("body", 11.0, 72.0, 700.0)],
        vec![("Second", 24.0, 72.0, 720.0), ("body", 11.0, 72.0, 700.0)],
    ]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");

    let root = reopened
        .catalog()
        .expect("catalogue")
        .get(b"Outlines")
        .expect("Outlines")
        .as_reference()
        .expect("référence");
    let mut titles = Vec::new();
    let mut current = reopened
        .get_dictionary(root)
        .expect("racine")
        .get(b"First")
        .ok()
        .and_then(|o| o.as_reference().ok());
    while let Some(id) = current {
        let node = reopened.get_dictionary(id).expect("entrée");
        let title = node.get(b"Title").expect("titre");
        titles.push(decode_pdf_text(title.as_str().expect("octets")));
        current = node.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }
    assert_eq!(titles, vec!["First".to_owned(), "Second".to_owned()]);
}

#[test]
fn body_only_document_produces_no_bookmark() {
    let pdf = text_pdf(&[vec![
        ("just body", 11.0, 72.0, 700.0),
        ("more body", 11.0, 72.0, 680.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let written = doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(written, 0, "aucun titre au-dessus de la taille du corps");
    assert!(doc.outline().is_empty());
}

#[test]
fn clear_outline_empties_the_tree() {
    let pdf = text_pdf(&[vec![
        ("Heading", 24.0, 72.0, 720.0),
        ("body", 11.0, 72.0, 700.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(doc.outline().len(), 1);

    let removed = doc.clear_outline().expect("suppression");
    assert_eq!(removed, 1, "une entrée retirée");
    assert!(doc.outline().is_empty(), "le plan doit être vide");

    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");
    assert!(
        reopened.catalog().expect("catalogue").get(b"Outlines").is_err(),
        "la clé /Outlines doit disparaître du catalogue"
    );
}

#[test]
fn max_entries_caps_the_number_of_bookmarks() {
    let pdf = text_pdf(&[vec![
        ("H1", 24.0, 72.0, 740.0),
        ("H2", 24.0, 72.0, 710.0),
        ("H3", 24.0, 72.0, 680.0),
        ("body one", 11.0, 72.0, 650.0),
        ("body two", 11.0, 72.0, 630.0),
        ("body three", 11.0, 72.0, 610.0),
        ("body four", 11.0, 72.0, 590.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let style = OutlineStyle {
        max_entries: 2,
        ..OutlineStyle::default()
    };
    let written = doc.generate_outline(&style).expect("plan");
    assert_eq!(written, 2, "le plafond s'applique");
    assert_eq!(doc.outline().len(), 2);
}

#[test]
fn generated_title_decodes_back_to_the_heading_text() {
    let pdf = text_pdf(&[vec![
        ("Café Général", 24.0, 72.0, 720.0),
        ("body", 11.0, 72.0, 700.0),
        ("body again", 11.0, 72.0, 680.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].title, "Café Général");
}
