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

/// PDF d'une page dont le contenu est donné brut : le flux de contenu est
/// écrit tel quel, ce qui permet de placer précisément le texte.
fn document_avec_contenu(contenu: &str) -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let stream = doc.add_object(Stream::new(dictionary! {}, contenu.as_bytes().to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => stream,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page)]),
        "Count" => 1,
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

/// PDF d'une page contenant une image de 2×2 pixels.
fn document_avec_image() -> Vec<u8> {
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
    bytes
}

/// Noms de toutes les parties de l'archive.
fn noms_des_parts(archive: &mut ZipArchive<std::fs::File>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|index| archive.by_index(index).ok().map(|f| f.name().to_string()))
        .collect()
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

/// Le mode « flux » reconnaît un titre plus gros que le corps.
#[test]
fn word_flow_detects_headings() {
    // Le titre est écrit en 36 pt, le corps en 12 pt : le rapport est de 3,
    // ce qui place le paragraphe en Heading1.
    let bytes = document_avec_contenu(
        "BT /F1 36 Tf 60 700 Td (Titre) Tj ET BT /F1 12 Tf 60 660 Td (Corps) Tj ET",
    );
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-word-headings.docx");
    doc.export_office(&out_path, OfficeFormat::Word)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let document = lire_part(&mut archive, "word/document.xml");
    assert!(
        document.contains(r#"<w:pStyle w:val="Heading1"/>"#),
        "titre non détecté : {document}"
    );
    let styles = lire_part(&mut archive, "word/styles.xml");
    assert!(
        styles.contains(r#"w:styleId="Heading1""#)
            && styles.contains(r#"<w:basedOn w:val="Normal"/>"#),
        "style Heading1 non défini : {styles}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// Deux lignes proches forment un paragraphe ; une ligne lointaine, un autre.
#[test]
fn word_flow_keeps_paragraphs() {
    // Deux lignes à 14 pt d'interligne : elles partagent le paragraphe.
    // La troisième, 200 pt plus bas, ouvre un nouveau paragraphe.
    let bytes = document_avec_contenu(
        "BT /F1 12 Tf 60 700 Td (Ligne une) Tj ET \
         BT /F1 12 Tf 60 686 Td (Ligne deux) Tj ET \
         BT /F1 12 Tf 60 486 Td (Ligne trois) Tj ET",
    );
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-word-paragraphes.docx");
    doc.export_office(&out_path, OfficeFormat::Word)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let document = lire_part(&mut archive, "word/document.xml");
    assert!(
        document.contains("<w:br/>"),
        "aucun saut de ligne dans le paragraphe : {document}"
    );
    // Deux paragraphes de texte : le second commence après le premier.
    let first = document.find("Ligne une").expect("première ligne");
    let second = document.find("Ligne deux").expect("deuxième ligne");
    let third = document.find("Ligne trois").expect("troisième ligne");
    assert!(first < second && second < third, "ordre des lignes : {document}");
    // Le saut de ligne est entre les deux premières lignes, pas après la
    // troisième : le paragraphe couvre « une » et « deux ».
    let br = document.find("<w:br/>").expect("saut de ligne");
    assert!(br > first && br < third, "saut de ligne mal placé : {document}");
    let _ = std::fs::remove_file(out_path);
}

/// La mise en page exacte utilise la vraie boîte du fragment.
#[test]
fn word_exact_layout_uses_real_geometry() {
    use youpdf_core::office::OfficeLayout;

    // Texte posé en (100, 600), corps 20 : la boîte est
    // [100, 600, 100 + largeur, 620].
    let bytes = document_avec_contenu("BT /F1 20 Tf 100 600 Td (Hello) Tj ET");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-word-geometrie.docx");
    doc.export_office_layout(&out_path, OfficeFormat::Word, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let document = lire_part(&mut archive, "word/document.xml");
    // `w:x` = 100 pt × 20 = 2000 twips.
    assert!(
        document.contains(r#"w:x="2000""#),
        "abscisse absente : {document}"
    );
    // `w:y` = (792 − 620) × 20 = 3440 twips (ancré depuis le haut).
    assert!(
        document.contains(r#"w:y="3440""#),
        "ordonnée absente : {document}"
    );
    // La hauteur reprend la boîte du fragment : 20 pt × 20 = 400 twips.
    assert!(
        document.contains(r#"w:h="400""#),
        "hauteur absente : {document}"
    );
    // Le corps est écrit en demi-points : 20 pt → 40.
    assert!(
        document.contains(r#"<w:sz w:val="40"/>"#),
        "corps absent : {document}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// La mise en page exacte pose les images à leur position.
#[test]
fn word_exact_places_images() {
    use youpdf_core::office::OfficeLayout;

    let doc = Document::from_bytes(&document_avec_image()).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-word-images-exact.docx");
    doc.export_office_layout(&out_path, OfficeFormat::Word, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let names = noms_des_parts(&mut archive);
    assert!(
        names.iter().any(|name| name == "word/media/image1.png"),
        "image absente : {names:?}"
    );
    let document = lire_part(&mut archive, "word/document.xml");
    assert!(document.contains("<w:drawing>"), "dessin absent : {document}");
    // L'image est posée en 100 × 100 pt : 100 × 12 700 = 1 270 000 EMU.
    assert!(
        document.contains(r#"<wp:extent cx="1270000" cy="1270000"/>"#),
        "taille de l'image absente : {document}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// Les cellules portent leur type : nombre, date, texte.
#[test]
fn excel_detects_number_and_date_cells() {
    use youpdf_core::office::OfficeLayout;

    let bytes = document_avec_contenu(
        "BT /F1 12 Tf 50 700 Td (Nom) Tj ET \
         BT /F1 12 Tf 200 700 Td (Valeur) Tj ET \
         BT /F1 12 Tf 200 680 Td (1234.56) Tj ET \
         BT /F1 12 Tf 350 680 Td (2024-01-15) Tj ET",
    );
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-excel-types.xlsx");
    doc.export_office_layout(&out_path, OfficeFormat::Excel, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let sheet = lire_part(&mut archive, "xl/worksheets/sheet1.xml");
    // Un nombre ne porte pas d'attribut de type.
    assert!(
        sheet.contains(r#"<c r="B2"><v>1234.56</v></c>"#),
        "cellule numérique absente : {sheet}"
    );
    // Une date porte un index de style.
    assert!(
        sheet.contains(r#"<c r="C2" s="1"><v>"#),
        "cellule de date absente : {sheet}"
    );
    let names = noms_des_parts(&mut archive);
    assert!(
        names.iter().any(|name| name == "xl/styles.xml"),
        "styles absents : {names:?}"
    );
    let styles = lire_part(&mut archive, "xl/styles.xml");
    assert!(
        styles.contains(r#"numFmtId="14""#),
        "format de date absent : {styles}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// Les colonnes reçoivent une largeur calée sur leur contenu.
#[test]
fn excel_sets_column_widths() {
    use youpdf_core::office::OfficeLayout;

    let bytes = document_avec_contenu(
        "BT /F1 12 Tf 50 700 Td (Nom) Tj ET BT /F1 12 Tf 250 700 Td (Prix) Tj ET",
    );
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-excel-colonnes.xlsx");
    doc.export_office_layout(&out_path, OfficeFormat::Excel, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let sheet = lire_part(&mut archive, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<cols>"), "largeurs absentes : {sheet}");
    assert!(sheet.contains("<col "), "colonne sans largeur : {sheet}");
    let _ = std::fs::remove_file(out_path);
}

/// La diapositive reprend la taille de la page PDF, en EMU.
#[test]
fn powerpoint_places_text_shapes() {
    use youpdf_core::office::OfficeLayout;

    let bytes = document_avec_contenu("BT /F1 20 Tf 100 600 Td (Hello) Tj ET");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-pptx-formes.pptx");
    doc.export_office_layout(&out_path, OfficeFormat::PowerPoint, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let presentation = lire_part(&mut archive, "ppt/presentation.xml");
    // 612 pt × 12 700 = 7 772 400 EMU ; 792 × 12 700 = 10 058 400 EMU.
    assert!(
        presentation.contains(r#"<p:sldSz cx="7772400" cy="10058400"/>"#),
        "taille de diapositive absente : {presentation}"
    );
    let slide = lire_part(&mut archive, "ppt/slides/slide1.xml");
    // La forme est posée en x = 100 pt = 1 270 000 EMU.
    assert!(
        slide.contains(r#"<a:off x="1270000" y="2184400"/>"#),
        "position absente : {slide}"
    );
    assert!(slide.contains("<p:sp>"), "aucune forme de texte : {slide}");
    assert!(
        slide.contains(r#"<a:rPr lang="fr-FR" sz="2000""#),
        "corps absent : {slide}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// Les images sont embarquées et posées sur la diapositive.
#[test]
fn powerpoint_embeds_images() {
    use youpdf_core::office::OfficeLayout;

    let doc = Document::from_bytes(&document_avec_image()).expect("ouverture");
    let out_path = std::env::temp_dir().join("youpdf-pptx-images.pptx");
    doc.export_office_layout(&out_path, OfficeFormat::PowerPoint, OfficeLayout::Exact)
        .expect("export");

    let mut archive = ouvrir(&out_path);
    let names = noms_des_parts(&mut archive);
    assert!(
        names.iter().any(|name| name == "ppt/media/image1.png"),
        "image absente : {names:?}"
    );
    let slide = lire_part(&mut archive, "ppt/slides/slide1.xml");
    assert!(slide.contains("<p:pic>"), "aucune image : {slide}");
    assert!(
        slide.contains(r#"<a:blip r:embed="rId2"/>"#),
        "relation image absente : {slide}"
    );
    let rels = lire_part(&mut archive, "ppt/slides/_rels/slide1.xml.rels");
    assert!(
        rels.contains("../media/image1.png"),
        "cible image absente : {rels}"
    );
    let types = lire_part(&mut archive, "[Content_Types].xml");
    assert!(
        types.contains(r#"<Default Extension="png" ContentType="image/png"/>"#),
        "type PNG absent : {types}"
    );
    let _ = std::fs::remove_file(out_path);
}

/// Toutes les parties XML des trois paquets sont bien formées.
#[test]
fn all_parts_are_well_formed_xml() {
    use youpdf_core::office::OfficeLayout;

    let bytes = document_avec_contenu(
        "BT /F1 24 Tf 60 700 Td (Titre & <balise>) Tj ET \
         BT /F1 12 Tf 60 660 Td (Corps de texte) Tj ET",
    );
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let images = Document::from_bytes(&document_avec_image()).expect("ouverture");

    for (format, layout, extension) in [
        (OfficeFormat::Word, OfficeLayout::Flow, "docx"),
        (OfficeFormat::Word, OfficeLayout::Exact, "docx"),
        (OfficeFormat::Excel, OfficeLayout::Flow, "xlsx"),
        (OfficeFormat::Excel, OfficeLayout::Exact, "xlsx"),
        (OfficeFormat::PowerPoint, OfficeLayout::Flow, "pptx"),
        (OfficeFormat::PowerPoint, OfficeLayout::Exact, "pptx"),
    ] {
        let out_path = std::env::temp_dir().join(format!(
            "youpdf-bien-forme-{extension}-{}.{extension}",
            match layout {
                OfficeLayout::Flow => "flux",
                OfficeLayout::Exact => "exact",
            }
        ));
        doc.export_office_layout(&out_path, format, layout)
            .expect("export");
        let mut archive = ouvrir(&out_path);
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("partie");
            let name = file.name().to_string();
            if !name.ends_with(".xml") && !name.ends_with(".rels") {
                continue;
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).expect("lecture");
            let xml = String::from_utf8(buf).expect("UTF-8");
            assert!(xml_bien_forme(&xml), "part {name} mal formée");
        }
        let _ = std::fs::remove_file(out_path);
    }

    // Le paquet avec image, lui aussi, est bien formé.
    let out_path = std::env::temp_dir().join("youpdf-bien-forme-image.docx");
    images
        .export_office_layout(&out_path, OfficeFormat::Word, OfficeLayout::Exact)
        .expect("export");
    let mut archive = ouvrir(&out_path);
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("partie");
        let name = file.name().to_string();
        if !name.ends_with(".xml") && !name.ends_with(".rels") {
            continue;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).expect("lecture");
        let xml = String::from_utf8(buf).expect("UTF-8");
        assert!(xml_bien_forme(&xml), "part {name} mal formée");
    }
    let _ = std::fs::remove_file(out_path);
}
