//! Compression / optimisation du document.
//!
//! Le fichier ré-écrit doit se relire, conserver son nombre de pages, et
//! porter des flux recompressés. La taille d'avant/après est rapportée.

use std::io::Write;
use std::path::PathBuf;

use flate2::write::ZlibEncoder;
use youpdf_core::compress::{CompressionLevel, CompressionOptions};
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

/// Une page 612×792 portant une image RVB 1600×1600 en `FlateDecode`.
///
/// Les échantillons sont pseudo-aléatoires : le flux `FlateDecode` est donc
/// volumineux et quasi incompressible, ce qui garantit que le réencodage JPEG
/// produit bien un flux plus petit.
fn pdf_with_large_flate_rgb_image() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let (width, height) = (1600u32, 1600u32);
    let mut raw = Vec::with_capacity((width * height * 3) as usize);
    let mut state: u32 = 12345;
    for _ in 0..(width * height * 3) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        raw.push((state >> 24) as u8);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).expect("compression zlib de test");
    let compressed = encoder.finish().expect("fin de compression zlib");

    let mut doc = LopdfDoc::with_version("1.7");
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
            "Filter" => Object::Name(b"FlateDecode".to_vec()),
        },
        compressed,
    ));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 612 0 0 612 0 0 cm /Im0 Do Q".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image } },
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
    doc.save_to(&mut out).expect("écriture du PDF image de test");
    out
}

#[test]
fn compresser_avec_reduction_image_screen() {
    let mut doc = Document::from_bytes(&pdf_with_large_flate_rgb_image()).expect("ouverture");
    let path = chemin("image-screen");
    let report = doc
        .compress_with(&path, &CompressionLevel::Screen.options())
        .expect("compression avec image");

    assert!(
        report.images_resampled >= 1,
        "aucune image rééchantillonnée"
    );
    assert!(
        report.after_bytes < report.before_bytes,
        "le fichier n'a pas maigri : {} -> {}",
        report.before_bytes,
        report.after_bytes
    );

    let bytes = std::fs::read(&path).expect("lecture");

    assert!(
        contient(&bytes, b"/DCTDecode"),
        "image non recompressée en JPEG"
    );
    assert!(
        !contient(&bytes, b"/Width 1600"),
        "largeur d'image non réduite"
    );

    let relu = Document::open(&path).expect("relecture");
    assert_eq!(relu.page_count(), 1, "le nombre de pages doit être conservé");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn compresser_avec_dpi_zero_ne_resample_rien() {
    let mut doc = Document::from_bytes(&pdf_with_large_flate_rgb_image()).expect("ouverture");
    let path = chemin("image-dpi-zero");
    let options = CompressionOptions {
        target_dpi: 0.0,
        jpeg_quality: 55,
        upscale: false,
    };
    let report = doc.compress_with(&path, &options).expect("compression");

    assert_eq!(report.images_resampled, 0, "aucune image ne doit changer");

    let bytes = std::fs::read(&path).expect("lecture");
    assert!(
        contient(&bytes, b"/Width 1600"),
        "l'image ne doit pas être rééchantillonnée"
    );

    let relu = Document::open(&path).expect("relecture");
    assert_eq!(relu.page_count(), 1, "le nombre de pages doit être conservé");
    let _ = std::fs::remove_file(&path);
}

/// Les optimisations structurelles retirent les pièces demandées.
#[test]
fn extras_retirent_plan_et_vignettes() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
    use youpdf_core::CompressionExtras;

    let mut raw = LopdfDoc::load_mem(&page_repetitive()).expect("chargement bas niveau");
    let catalog_id = raw.trailer.get(b"Root").and_then(Object::as_reference).expect("Root");
    let page_id = raw.get_pages().values().copied().next().expect("page");
    let outline = raw.add_object(dictionary! {
        "Type" => Object::Name(b"Outlines".to_vec()),
        "Count" => 0,
    });
    let metadata = raw.add_object(Stream::new(
        dictionary! { "Type" => Object::Name(b"Metadata".to_vec()) },
        b"<x/>".to_vec(),
    ));
    let thumb = raw.add_object(Stream::new(dictionary! {}, vec![0u8; 4]));
    if let Ok(dict) = raw.get_dictionary_mut(catalog_id) {
        dict.set("Outlines", Object::Reference(outline));
        dict.set("Metadata", Object::Reference(metadata));
    }
    if let Ok(dict) = raw.get_dictionary_mut(page_id) {
        dict.set("Thumb", Object::Reference(thumb));
    }
    let mut bytes = Vec::new();
    raw.save_to(&mut bytes).expect("écriture");

    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    let path = chemin("extras");
    let options = CompressionLevel::Ebook.options();
    let extras = CompressionExtras {
        strip_metadata: true,
        strip_thumbnails: true,
        strip_bookmarks: true,
        strip_structure: false,
    };
    doc.compress_with_extras(&path, &options, &extras).expect("compression");

    let relu = Document::open(&path).expect("relecture");
    let raw = LopdfDoc::load_mem(&relu.to_bytes().expect("sérialisation")).expect("relecture brute");
    let catalog = raw.catalog().expect("catalogue");
    assert!(!catalog.has(b"Outlines"), "le plan devait disparaître");
    assert!(!catalog.has(b"Metadata"), "les métadonnées devaient disparaître");
    let page = raw.get_pages().values().copied().next().expect("page");
    assert!(
        !raw.get_dictionary(page).expect("page").has(b"Thumb"),
        "la vignette devait disparaître"
    );
    let _ = std::fs::remove_file(&path);
}

/// L'estimation ne modifie pas le document et renvoie une taille plausible.
#[test]
fn estimation_ne_modifie_rien() {
    use youpdf_core::CompressionExtras;

    let mut doc = Document::from_bytes(&page_repetitive()).expect("ouverture");
    let before = doc.to_bytes().expect("sérialisation").len();
    let options = CompressionLevel::Screen.options();
    let estimate = doc
        .estimate_compressed(&options, &CompressionExtras::default())
        .expect("estimation");
    assert!(estimate > 0);
    assert_eq!(
        doc.to_bytes().expect("sérialisation").len(),
        before,
        "le document d'origine doit rester intact"
    );
}
