//! Tests d'export PDF vers Word, Excel et PowerPoint.
//!
//! Chaque format produit une archive ZIP dont on vérifie la présence des
//! parties OOXML attendues, et dont chaque partie XML est relue avec
//! `quick-xml` pour confirmer qu'elle est bien formée.

use std::io::Read;

use youpdf_core::{Document, OfficeFormat};
use zip::ZipArchive;

/// PDF en mémoire : une page par texte fourni.
fn document_avec(textes: &[&str]) -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let mut pages = Vec::new();
    for text in textes {
        let content = doc.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 24 Tf 60 700 Td ({text}) Tj ET").into_bytes(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        pages.push(page);
    }
    let kids: Vec<Object> = pages.into_iter().map(Object::Reference).collect();
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => textes.len() as i64,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Vrai si `xml` est bien formé (balises correctement appariées).
fn xml_bien_forme(xml: &str) -> bool {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

/// Lit une partie de l'archive, vérifie qu'elle est non vide et bien formée.
fn lire_part(archive: &mut ZipArchive<std::fs::File>, name: &str) -> String {
    let mut file = archive
        .by_name(name)
        .unwrap_or_else(|e| panic!("part {name} introuvable : {e}"));
    assert!(file.size() > 0, "part {name} vide");
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).expect("lecture de la part");
    let xml = String::from_utf8(buf).expect("part encodée en UTF-8");
    assert!(xml_bien_forme(&xml), "part {name} mal formée : {xml}");
    xml
}

fn ouvrir(path: &std::path::Path) -> ZipArchive<std::fs::File> {
    assert!(path.exists(), "fichier créé : {}", path.display());
    let file = std::fs::File::open(path).expect("ouverture ZIP");
    ZipArchive::new(file).expect("lecture ZIP")
}

#[test]
fn word_export_basic() {
    let doc = Document::from_bytes(&document_avec(&[
        "Hello World",
        "This & that",
        "<tags> are <bad>",
    ]))
    .expect("ouverture");

    let out_path = std::env::temp_dir().join("test_word.docx");
    doc.export_office(&out_path, OfficeFormat::Word)
        .expect("export Word");
    assert!(
        out_path.metadata().unwrap().len() > 0,
        "fichier Word non vide"
    );

    let mut archive = ouvrir(&out_path);
    let expected_parts = [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/_rels/document.xml.rels",
    ];
    for part in &expected_parts {
        lire_part(&mut archive, part);
    }

    // Le texte est échappé, pas injecté brut.
    let document = lire_part(&mut archive, "word/document.xml");
    assert!(document.contains("Hello World"), "texte conservé");
    assert!(document.contains("&amp;"), "esperluette échappée");
    assert!(document.contains("&lt;tags&gt;"), "chevrons échappés");
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn excel_export_basic() {
    let doc = Document::from_bytes(&document_avec(&[
        "Row 1 data",
        "Row 2 data with & special",
        "Third row",
    ]))
    .expect("ouverture");

    let out_path = std::env::temp_dir().join("test_excel.xlsx");
    doc.export_office(&out_path, OfficeFormat::Excel)
        .expect("export Excel");
    assert!(
        out_path.metadata().unwrap().len() > 0,
        "fichier Excel non vide"
    );

    let mut archive = ouvrir(&out_path);
    let expected_parts = [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "xl/sharedStrings.xml",
    ];
    for part in &expected_parts {
        lire_part(&mut archive, part);
    }

    let strings = lire_part(&mut archive, "xl/sharedStrings.xml");
    assert!(strings.contains("Row 1 data"), "contenu dans sharedStrings");
    assert!(strings.contains("&amp;"), "esperluette échappée");
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn powerpoint_export_basic() {
    let doc = Document::from_bytes(&document_avec(&[
        "Slide 1 Title",
        "Slide 1 body text with & accenté",
        "Slide 2 Title",
        "Slide 2 body",
    ]))
    .expect("ouverture");

    let out_path = std::env::temp_dir().join("test_pptx.pptx");
    doc.export_office(&out_path, OfficeFormat::PowerPoint)
        .expect("export PowerPoint");
    assert!(
        out_path.metadata().unwrap().len() > 0,
        "fichier PowerPoint non vide"
    );

    let mut archive = ouvrir(&out_path);
    let expected_parts = [
        "[Content_Types].xml",
        "_rels/.rels",
        "ppt/presentation.xml",
        "ppt/_rels/presentation.xml.rels",
        "ppt/slideMasters/slideMaster1.xml",
        "ppt/slideLayouts/slideLayout1.xml",
        "ppt/theme/theme1.xml",
        "ppt/slides/slide1.xml",
        "ppt/slides/_rels/slide1.xml.rels",
    ];
    for part in &expected_parts {
        lire_part(&mut archive, part);
    }

    // Une diapositive par page PDF : les quatre pages produisent quatre
    // diapositives, référencées par la présentation.
    for number in 1..=4 {
        lire_part(&mut archive, &format!("ppt/slides/slide{number}.xml"));
    }
    let presentation = lire_part(&mut archive, "ppt/presentation.xml");
    assert!(
        presentation.contains("rId2") && presentation.contains("rId5"),
        "les diapositives sont référencées"
    );
    let _ = std::fs::remove_file(&out_path);
}

#[test]
fn extensions() {
    for (format, ext) in [
        (OfficeFormat::Word, "docx"),
        (OfficeFormat::Excel, "xlsx"),
        (OfficeFormat::PowerPoint, "pptx"),
    ] {
        assert_eq!(format.extension(), ext, "extension de {format:?}");
    }
}
