//! Extraction de texte via pdfium.
//!
//! pdfium résout les polices CID que le décodeur de flux ne sait pas lire :
//! c'est lui qui alimente la sélection de texte. Les tests sautent quand la
//! bibliothèque pdfium est absente de la machine.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::{Document, PdfiumRenderer, Renderer};

/// PDF d'une page portant une ligne de texte, police Helvetica standard.
fn pdf_avec_texte(texte: &str) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = format!(
        "BT /F1 12 Tf 1 0 0 1 72 700 Tm ({texte}) Tj ET\n"
    );
    let stream = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => stream,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// pdfium renvoie le texte de la page, avec un rectangle en repère page.
#[test]
fn pdfium_renvoie_le_texte_et_son_rectangle() {
    let Ok(renderer) = PdfiumRenderer::load() else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    let doc = Document::from_bytes(&pdf_avec_texte("Bonjour le monde")).expect("ouverture");
    let fragments = renderer.page_fragments(&doc, 1).expect("fragments");

    let texte: String = fragments.iter().map(|f| f.text.as_str()).collect();
    assert!(texte.contains("Bonjour"), "texte extrait : {texte:?}");

    let fragment = fragments
        .iter()
        .find(|f| f.text.contains("Bonjour"))
        .expect("fragment attendu");
    assert_eq!(fragment.page, 1);
    // Repère page, origine en bas à gauche : le bloc est dans la page.
    let [x1, y1, x2, y2] = fragment.rect;
    assert!(x1 >= 0.0 && y1 >= 0.0 && x2 > x1 && y2 > y1, "rect : {:?}", fragment.rect);
}
