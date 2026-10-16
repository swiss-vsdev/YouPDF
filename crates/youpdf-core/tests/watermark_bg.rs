//! Arrière-plans de page et numérotation Bates.
//!
//! Comme les filigranes, ces décorations sont écrites dans le flux de
//! contenu : ces tests vérifient que l'arrière-plan passe **avant** le
//! contenu d'origine, que l'image pose bien sa ressource, que le libellé
//! Bates est complété par des zéros au bon décalage, et que chaque retrait
//! ne touche que ses propres flux.

use youpdf_core::watermark::{Background, Bates, BatesPosition};
use youpdf_core::{Document, PageRange, Watermark};

/// `n` pages 612×792 dont le contenu d'origine porte `ORIGINAL`.
fn document_avec_contenu(n: usize) -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids = Vec::new();
    for _ in 0..n {
        let content = doc.add_object(Stream::new(dictionary! {}, b"ORIGINAL\n".to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => content,
            "Resources" => dictionary! {},
        });
        kids.push(page.into());
    }

    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => n as i64,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Flux de contenu d'une page (1-indexée), dans l'ordre du tableau.
fn content_streams(bytes: &[u8], page_num: u32) -> Vec<Vec<u8>> {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    let mut out = Vec::new();
    let Some(id) = doc.get_pages().get(&page_num).copied() else {
        return out;
    };
    let page = doc.get_dictionary(id).expect("page");
    let Ok(contents) = page.get(b"Contents") else {
        return out;
    };
    let mut push = |obj: &lopdf::Object| {
        if let Ok(stream) = obj.as_reference().and_then(|id| doc.get_object(id)).and_then(|o| o.as_stream()) {
            out.push(stream.content.clone());
        }
    };
    match contents {
        lopdf::Object::Array(arr) => arr.iter().for_each(&mut push),
        single => push(single),
    }
    out
}

/// Contenu concaténé d'une page, vu comme texte ASCII.
fn text_of(bytes: &[u8], page_num: u32) -> String {
    let mut joined = Vec::new();
    for stream in content_streams(bytes, page_num) {
        joined.extend_from_slice(&stream);
    }
    String::from_utf8_lossy(&joined).to_string()
}

/// Un petit PNG RGB sur disque, pour l'arrière-plan image.
fn ecrire_png() -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("youpdf_bg_{}_{nanos}.png", std::process::id()));
    let image = image::RgbImage::from_pixel(8, 8, image::Rgb([10, 20, 30]));
    image.save(&path).expect("écriture du PNG de test");
    path
}

/// L'arrière-plan couleur passe avant le contenu d'origine, qui reste après.
#[test]
fn arriere_plan_couleur_precede_contenu() {
    let mut doc = Document::from_bytes(&document_avec_contenu(1)).expect("ouverture");
    let bg = Background {
        color: [200, 10, 20],
        ..Default::default()
    };

    let posees = doc.add_background(&bg).expect("arrière-plan");
    assert_eq!(posees, 1);

    let bytes = doc.to_bytes().expect("écriture");
    let streams = content_streams(&bytes, 1);
    assert_eq!(streams.len(), 2, "un flux de fond + le contenu d'origine");

    let fond = String::from_utf8_lossy(&streams[0]).to_string();
    let origine = String::from_utf8_lossy(&streams[1]).to_string();
    assert!(fond.contains("re f"), "l'aplat doit remplir la boîte : {fond:?}");
    assert!(fond.contains("0.784"), "la couleur doit être écrite : {fond:?}");
    assert!(!fond.contains("ORIGINAL"), "le fond ne contient pas le texte");
    assert!(origine.contains("ORIGINAL"), "le contenu d'origine suit le fond");

    // Le flux de fond porte bien sa clé de repérage.
    let reloaded = lopdf::Document::load_mem(&bytes).expect("relecture");
    let page_id = reloaded.get_pages()[&1];
    let page = reloaded.get_dictionary(page_id).expect("page");
    let first = page
        .get(b"Contents")
        .and_then(lopdf::Object::as_array)
        .expect("tableau de contenus")[0]
        .as_reference()
        .expect("référence");
    let stream = reloaded.get_object(first).and_then(|o| o.as_stream()).expect("flux");
    assert!(stream.dict.get(b"YouPdfBackground").is_ok(), "clé absente");
}

/// L'arrière-plan image pose son XObject et le peint, recadré sur la boîte.
#[test]
fn arriere_plan_image_pose_la_ressource() {
    let png = ecrire_png();
    let mut doc = Document::from_bytes(&document_avec_contenu(1)).expect("ouverture");
    let bg = Background {
        image: Some(png.clone()),
        ..Default::default()
    };

    let posees = doc.add_background(&bg).expect("arrière-plan");
    assert_eq!(posees, 1);
    let _ = std::fs::remove_file(&png);

    let bytes = doc.to_bytes().expect("écriture");
    let text = text_of(&bytes, 1);
    assert!(text.contains("/YouPdfBG0 Do"), "image non peinte : {text:?}");
    assert!(text.contains("re W n"), "recadrage absent : {text:?}");

    let reloaded = lopdf::Document::load_mem(&bytes).expect("relecture");
    let page_id = reloaded.get_pages()[&1];
    let page = reloaded.get_dictionary(page_id).expect("page");
    let xobjects = page
        .get(b"Resources")
        .and_then(lopdf::Object::as_dict)
        .and_then(|r| r.get(b"XObject"))
        .and_then(lopdf::Object::as_dict)
        .expect("dictionnaire XObject");
    let id = xobjects
        .get(b"YouPdfBG0")
        .and_then(lopdf::Object::as_reference)
        .expect("référence YouPdfBG0");
    let image = reloaded.get_object(id).and_then(|o| o.as_stream()).expect("image");
    assert_eq!(
        image.dict.get(b"Subtype").and_then(lopdf::Object::as_name_str).ok(),
        Some("Image")
    );
}

/// `clear_backgrounds` ne retire que les flux d'arrière-plan.
#[test]
fn clear_backgrounds_retire_seulement_les_fonds() {
    let mut doc = Document::from_bytes(&document_avec_contenu(1)).expect("ouverture");
    doc.add_text_watermark(&Watermark::default())
        .expect("filigrane");
    doc.add_background(&Background {
        color: [10, 20, 30],
        ..Default::default()
    })
    .expect("fond 1");
    doc.add_background(&Background {
        color: [40, 50, 60],
        ..Default::default()
    })
    .expect("fond 2");

    let retires = doc.clear_backgrounds().expect("retrait");
    assert_eq!(retires, 2, "les deux fonds seulement");

    let bytes = doc.to_bytes().expect("écriture");
    let text = text_of(&bytes, 1);
    assert!(!text.contains("re f"), "les fonds devaient disparaître");
    assert!(text.contains("ORIGINAL"), "le contenu d'origine reste");
    assert!(text.contains("CONFIDENTIEL"), "le filigrane reste");
}

/// Le libellé Bates est complété par des zéros et démarre à `start`.
#[test]
fn bates_zero_padding_et_decalage() {
    let mut doc = Document::from_bytes(&document_avec_contenu(2)).expect("ouverture");
    let b = Bates {
        prefix: "DOC-".to_string(),
        start: 5,
        digits: 4,
        position: BatesPosition::BottomRight,
        pages: PageRange::All,
        ..Default::default()
    };

    let numerotees = doc.add_bates(&b).expect("Bates");
    assert_eq!(numerotees, 2);

    let bytes = doc.to_bytes().expect("écriture");
    assert!(text_of(&bytes, 1).contains("(DOC-0005) Tj"), "1re page");
    assert!(text_of(&bytes, 2).contains("(DOC-0006) Tj"), "2e page");
}

/// `clear_bates` ne retire que les flux Bates.
#[test]
fn clear_bates_retire_seulement_bates() {
    let mut doc = Document::from_bytes(&document_avec_contenu(1)).expect("ouverture");
    doc.add_text_watermark(&Watermark::default())
        .expect("filigrane");
    doc.add_bates(&Bates::default()).expect("Bates");

    let retires = doc.clear_bates().expect("retrait");
    assert_eq!(retires, 1);

    let bytes = doc.to_bytes().expect("écriture");
    let text = text_of(&bytes, 1);
    assert!(!text.contains("(000001) Tj"), "le Bates devait disparaître");
    assert!(text.contains("CONFIDENTIEL"), "le filigrane reste");
    assert!(text.contains("ORIGINAL"), "le contenu d'origine reste");
}
