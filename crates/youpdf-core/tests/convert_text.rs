//! Export de la couche texte (texte brut, Markdown, page web).

use youpdf_core::convert::TextFormat;
use youpdf_core::Document;

/// Deux pages contenant du texte, dont des caractères à échapper.
fn deux_pages() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let mut pages = Vec::new();
    for text in ["Alpha & <b>", "Beta"] {
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
        "Count" => 2,
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

/// Le texte brut garde les pages et n'échappe rien.
#[test]
fn export_texte_brut() {
    let doc = Document::from_bytes(&deux_pages()).expect("ouverture");
    let text = doc.export_text(TextFormat::Plain).expect("export");

    assert!(text.contains("Alpha & <b>"), "texte manquant : {text:?}");
    assert!(text.contains("Beta"), "deuxième page manquante : {text:?}");
    assert!(!text.contains("&amp;"), "le brut ne doit pas échapper");
    assert_eq!(TextFormat::Plain.extension(), "txt");
}

/// Le Markdown titre le document et chaque page.
#[test]
fn export_markdown() {
    let doc = Document::from_bytes(&deux_pages()).expect("ouverture");
    let text = doc.export_text(TextFormat::Markdown).expect("export");

    assert!(text.starts_with("# "), "pas de titre : {text:?}");
    assert!(text.contains("## Page 1"), "titre de page 1 : {text:?}");
    assert!(text.contains("## Page 2"), "titre de page 2 : {text:?}");
    assert_eq!(TextFormat::Markdown.extension(), "md");
}

/// La page web échappe le texte et sépare les pages.
#[test]
fn export_html() {
    let doc = Document::from_bytes(&deux_pages()).expect("ouverture");
    let text = doc.export_text(TextFormat::Html).expect("export");

    assert!(text.contains("<!DOCTYPE html>"), "pas d'en-tête : {text:?}");
    assert!(text.contains("charset=\"utf-8\""), "pas d'encodage : {text:?}");
    assert!(text.contains("id=\"page-2\""), "page 2 absente : {text:?}");
    assert!(text.contains("&amp;"), "& non échappé : {text:?}");
    assert!(text.contains("&lt;b&gt;"), "balise non échappée : {text:?}");
    assert!(text.contains("</html>"), "document non fermé : {text:?}");
    assert_eq!(TextFormat::Html.extension(), "html");
}

/// Les noms affichés portent l'extension, comme dans la liste de formats.
#[test]
fn noms_affiches() {
    for (format, needle) in [
        (TextFormat::Plain, ".txt"),
        (TextFormat::Markdown, ".md"),
        (TextFormat::Html, ".html"),
    ] {
        assert!(
            format.display_name().contains(needle),
            "{} ne contient pas {needle}",
            format.display_name()
        );
    }
}
