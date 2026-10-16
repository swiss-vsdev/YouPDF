//! Export de la couche texte enrichie (CSV, XML, RTF) et extensions.

use youpdf_core::convert::{ImageFormat, TextFormat};
use youpdf_core::Document;

/// PDF en mémoire : une page par texte fourni, même construction que
/// `tests/convert_text.rs`.
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

/// Le CSV cite un champ contenant une virgule et un guillemet (RFC 4180).
#[test]
fn csv_guillemets() {
    let doc = Document::from_bytes(&document_avec(&["He said \"hi\", ok"])).expect("ouverture");
    let text = doc.export_text(TextFormat::Csv).expect("export");

    assert!(
        text.starts_with("page,line,text\n"),
        "en-tête CSV manquant : {text:?}"
    );
    assert!(
        text.contains("\"He said \"\"hi\"\", ok\""),
        "champ non cité ou guillemets non doublés : {text:?}"
    );
}

/// Le XML échappe `&` et `<`.
#[test]
fn xml_echappement() {
    let doc = Document::from_bytes(&document_avec(&["A & B < C"])).expect("ouverture");
    let text = doc.export_text(TextFormat::Xml).expect("export");

    assert!(
        text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        "déclaration XML manquante : {text:?}"
    );
    assert!(text.contains("<document>"), "racine manquante : {text:?}");
    assert!(
        text.contains("<line>A &amp; B &lt; C</line>"),
        "ligne non échappée : {text:?}"
    );
    assert!(text.contains("</document>"), "racine non fermée : {text:?}");
}

/// Le RTF échappe l'accolade et encode le non-ASCII en `\uN?`.
#[test]
fn rtf_echappement() {
    let doc = Document::from_bytes(&document_avec(&["Brace { et caf\u{e9}"])).expect("ouverture");
    let text = doc.export_text(TextFormat::Rtf).expect("export");

    assert!(
        text.starts_with("{\\rtf1\\ansi\\deff0"),
        "en-tête RTF manquant : {text:?}"
    );
    assert!(
        text.contains("\\{"),
        "accolade non échappée : {text:?}"
    );
    assert!(
        text.contains("\\u"),
        "caractère non ASCII non encodé : {text:?}"
    );
    assert!(text.ends_with('}'), "RTF non fermé : {text:?}");
}

/// Chaque format annonce la bonne extension.
#[test]
fn extensions() {
    for (format, ext) in [
        (TextFormat::Plain, "txt"),
        (TextFormat::Markdown, "md"),
        (TextFormat::Html, "html"),
        (TextFormat::Csv, "csv"),
        (TextFormat::Xml, "xml"),
        (TextFormat::Rtf, "rtf"),
    ] {
        assert_eq!(format.extension(), ext, "extension de {format:?}");
    }

    assert_eq!(ImageFormat::Png.extension(), "png");
    assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
}
