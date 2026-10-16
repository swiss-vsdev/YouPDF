//! Compression / optimisation du document.
//!
//! Le fichier ré-écrit doit se relire, conserver son nombre de pages, et
//! porter des flux recompressés. La taille d'avant/après est rapportée.

use std::path::PathBuf;

use youpdf_core::{CompressionReport, Document};

/// Une page 612×792 dont le contenu est très répétitif : c'est ce qui rend
/// la recompression `FlateDecode` réellement avantageuse.
fn page_repetitive() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let mut contenu = Vec::new();
    for _ in 0..200 {
        contenu.extend_from_slice(b"BT /F1 24 Tf 60 700 Td (YouPDF) Tj ET\n");
    }
    let content = doc.add_object(Stream::new(dictionary! {}, contenu));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

fn chemin(nom: &str) -> PathBuf {
    std::env::temp_dir().join(format!("youpdf-compress-{nom}-{}.pdf", std::process::id()))
}

/// Vrai si `foin` contient la suite d'octets `motif`.
fn contient(foin: &[u8], motif: &[u8]) -> bool {
    foin.windows(motif.len()).any(|f| f == motif)
}

#[test]
fn compresser_reouvre_et_conserve_les_pages() {
    let mut doc = Document::from_bytes(&page_repetitive()).expect("ouverture");
    let path = chemin("pages");
    let report: CompressionReport = doc.compress(&path).expect("compression");

    assert!(report.before_bytes > 0, "taille avant nulle");
    let sur_disque = std::fs::metadata(&path).expect("taille du fichier").len() as usize;
    assert_eq!(report.after_bytes, sur_disque, "taille après incohérente");
    assert_eq!(
        report.saved_bytes(),
        report.before_bytes as isize - report.after_bytes as isize
    );

    let relu = Document::open(&path).expect("relecture");
    assert_eq!(relu.page_count(), 1, "le nombre de pages doit être conservé");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn compresser_recompresse_les_flux() {
    let mut doc = Document::from_bytes(&page_repetitive()).expect("ouverture");
    let path = chemin("flux");
    let report = doc.compress(&path).expect("compression");
    let bytes = std::fs::read(&path).expect("lecture");
    let _ = std::fs::remove_file(&path);

    // Le contenu répétitif doit être passé en `FlateDecode` : le marqueur
    // n'apparaît plus en clair et le fichier maigrit.
    assert!(contient(&bytes, b"/FlateDecode"), "flux non recompressés");
    assert!(!contient(&bytes, b"YouPDF"), "contenu encore en clair");
    assert!(report.saved_bytes() > 0, "aucun gain sur un contenu répétitif");
}
