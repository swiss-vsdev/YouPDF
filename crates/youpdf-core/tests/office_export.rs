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

/// La mise en page exacte écrit un cadre positionné par fragment.
#[test]
fn word_exact_layout_uses_frames() {
    use youpdf_core::office::OfficeLayout;

    let bytes = document_avec(&["Bonjour"]);
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-exact-layout.docx");
    doc.export_office_layout(&out_path, OfficeFormat::Word, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ZipArchive::new(std::fs::File::open(&out_path).expect("archive")).expect("zip");
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut xml)
        .expect("lecture");

    assert!(xml.contains("w:framePr"), "aucun cadre positionné : {xml}");
    assert!(xml.contains("w:hAnchor=\"page\""), "ancrage manquant");
    assert!(xml.contains("Bonjour"), "texte absent");
    let _ = std::fs::remove_file(out_path);
}

/// La détection de table place deux colonnes séparées sur la même ligne.
#[test]
fn excel_exact_layout_detects_columns() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
    use youpdf_core::office::OfficeLayout;

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 50 700 Td (Nom) Tj ET BT /F1 12 Tf 250 700 Td (Prix) Tj ET".to_vec(),
    ));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => pages_id,
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    }));
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");

    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-exact-table.xlsx");
    doc.export_office_layout(&out_path, OfficeFormat::Excel, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ZipArchive::new(std::fs::File::open(&out_path).expect("archive")).expect("zip");
    let mut sheet = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .expect("sheet1.xml")
        .read_to_string(&mut sheet)
        .expect("lecture");

    assert!(sheet.contains(r#"r="A1""#), "colonne A absente : {sheet}");
    assert!(sheet.contains(r#"r="B1""#), "colonne B absente : {sheet}");
    let _ = std::fs::remove_file(out_path);
}

/// Les images embarquées partent dans word/media et sont liées au document.
#[test]
fn word_embeds_page_images() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let mut image = Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => 2,
            "Height" => 2,
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => 8,
        },
        vec![255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
    );
    image.dict.set("Length", 12);
    let image_id = doc.add_object(image);
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 100 0 0 100 50 600 cm /Im0 Do Q".to_vec(),
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

    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-word-images.docx");
    doc.export_office(&out_path, OfficeFormat::Word)
        .expect("export");

    let mut archive = ZipArchive::new(std::fs::File::open(&out_path).expect("archive")).expect("zip");
    let names: Vec<String> = (0..archive.len())
        .filter_map(|index| archive.by_index(index).ok().map(|file| file.name().to_string()))
        .collect();
    assert!(
        names.iter().any(|name| name.starts_with("word/media/image1.")),
        "image absente : {names:?}"
    );
    let mut document = String::new();
    archive
        .by_name("word/document.xml")
        .expect("document.xml")
        .read_to_string(&mut document)
        .expect("lecture");
    assert!(document.contains("<w:drawing>"), "dessin absent");
    assert!(document.contains("r:embed=\"rId10\""), "relation image absente");
    let mut rels = String::new();
    archive
        .by_name("word/_rels/document.xml.rels")
        .expect("rels")
        .read_to_string(&mut rels)
        .expect("lecture");
    assert!(rels.contains("relationships/image"), "relation image absente : {rels}");
    let _ = std::fs::remove_file(out_path);
}
