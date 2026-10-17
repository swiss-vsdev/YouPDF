//! Insertion d'une image : décodage du disque puis pose sur une page.
//!
//! L'outil Image (mode Comment et mode Édition) passe par ces deux étapes :
//! `load_image_rgb` lit un PNG ou un JPEG, `insert_image_bitmap` écrit le
//! flux image dans le contenu de la page. Le test vérifie que PDFium voit
//! bien l'image posée, sinon la fonctionnalité ne serait qu'un bouton.

use youpdf_core::{
    edit::load_image_rgb, Document, PageBitmap, PdfiumRenderer, RenderOptions, Renderer,
};

/// Une page blanche 612×792 avec un peu de texte.
fn page_blanche() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 60 700 Td (YouPDF) Tj ET".to_vec(),
    ));
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

/// Pixels franchement rouges : la trace de l'image posée.
fn pixels_rouges(bitmap: &PageBitmap) -> usize {
    bitmap
        .pixels
        .chunks_exact(4)
        .filter(|px| px[0] as i32 - px[1] as i32 > 60 && px[0] as i32 - px[2] as i32 > 60)
        .count()
}

/// Rend la page 1 et compte les pixels rouges.
fn rendre(bytes: &[u8]) -> Option<usize> {
    let renderer = PdfiumRenderer::load().ok()?;
    let doc = Document::from_bytes(bytes).ok()?;
    let bitmap = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .ok()?;
    Some(pixels_rouges(&bitmap))
}

#[test]
fn une_image_decodee_se_pose_sur_la_page() {
    // Un PNG minuscule, entièrement rouge.
    let path = std::env::temp_dir().join(format!("youpdf-image-{}.png", std::process::id()));
    let mut img = image::RgbImage::new(8, 8);
    for px in img.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    img.save(&path).expect("écriture du PNG de test");

    let (pixels, width, height) = load_image_rgb(&path).expect("décodage du PNG");
    let _ = std::fs::remove_file(&path);
    assert_eq!((width, height), (8, 8), "dimensions du PNG");
    assert_eq!(pixels.len(), 8 * 8 * 3, "pixels RGB 8 bits");
    assert_eq!(&pixels[..3], &[220, 40, 40], "couleur du premier pixel");

    // Pose au quart de la page, puis contrôle visuel.
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.insert_image_bitmap(1, [150.0, 300.0, 350.0, 500.0], &pixels, width, height)
        .expect("insertion");
    let bytes = doc.to_bytes().expect("écriture");

    let Some(rouges) = rendre(&bytes) else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    assert!(
        rouges > 20_000,
        "l'image insérée n'apparaît pas au rendu ({rouges} pixels rouges)"
    );
}

/// L'extraction d'image embarquée doit rendre un PNG exploitable.
///
/// Le flux posé par `insert_image_bitmap` est du zlib d'échantillons bruts :
/// l'extraction doit donc décoder puis ré-encoder, pas recopier le flux.
#[test]
fn une_image_embarquee_s_extrait_en_png() {
    let mut img = image::RgbImage::new(8, 8);
    for px in img.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let raw = img.into_raw();

    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.insert_image_bitmap(1, [150.0, 300.0, 350.0, 500.0], &raw, 8, 8)
        .expect("insertion");

    let images = doc.extract_images(1).expect("liste des images");
    assert_eq!(images.len(), 1, "une image embarquée attendue");

    let png = doc.extract_image_png(&images[0]).expect("extraction PNG");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "signature PNG");

    let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .expect("relecture du PNG");
    assert_eq!(decoded.to_rgb8().dimensions(), (8, 8), "dimensions");
    assert_eq!(
        decoded.to_rgb8().into_raw()[..3],
        [220, 40, 40],
        "couleur du premier pixel"
    );
}
