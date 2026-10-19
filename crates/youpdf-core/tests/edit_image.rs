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

// ────────────────────────────────────────────────────────────────────────
// Édition d'une image déjà posée (tâche 3.4)
// ────────────────────────────────────────────────────────────────────────

use youpdf_core::{ImageCrop, ImageTransform};

/// Pixels franchement bleus : trace d'une image remplacée.
fn pixels_bleus(bitmap: &PageBitmap) -> usize {
    bitmap
        .pixels
        .chunks_exact(4)
        .filter(|px| px[2] as i32 - px[0] as i32 > 60 && px[2] as i32 - px[1] as i32 > 60)
        .count()
}

/// Rendu de la page 1 : (pixels rouges, pixels bleus).
fn rendre_couleurs(bytes: &[u8]) -> Option<(usize, usize)> {
    let renderer = PdfiumRenderer::load().ok()?;
    let doc = Document::from_bytes(bytes).ok()?;
    let bitmap = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .ok()?;
    Some((pixels_rouges(&bitmap), pixels_bleus(&bitmap)))
}

/// Document avec une image rouge de 8×8 posée au quart de la page.
fn document_avec_image(pixels: &[u8], width: u32, height: u32) -> Document {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.insert_image_bitmap(1, [150.0, 300.0, 350.0, 500.0], pixels, width, height)
        .expect("insertion");
    doc
}

/// Remplacer les pixels garde la place : le nom de ressource et la matrice du
/// flux de page ne bougent pas.
#[test]
fn remplacer_les_pixels_garde_la_place() {
    let mut rouge = image::RgbImage::new(8, 8);
    for px in rouge.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let mut doc = document_avec_image(&rouge.clone().into_raw(), 8, 8);

    let images = doc.extract_images(1).expect("liste des images");
    let avant = doc.extract_image_png(&images[0]).expect("PNG initial");

    let mut bleu = image::RgbImage::new(8, 8);
    for px in bleu.pixels_mut() {
        *px = image::Rgb([40, 40, 220]);
    }
    doc.replace_image_pixels(&images[0], &bleu.into_raw(), 8, 8)
        .expect("remplacement");

    // La ressource garde son nom et son objet.
    let apres = doc.extract_images(1).expect("liste des images");
    assert_eq!(apres.len(), 1);
    assert_eq!(apres[0].resource_name, images[0].resource_name);
    assert_eq!(apres[0].object_id, images[0].object_id);
    assert_ne!(
        doc.extract_image_png(&apres[0]).expect("PNG final"),
        avant,
        "les pixels n'ont pas changé"
    );

    let Some((rouges, bleus)) = rendre_couleurs(&doc.to_bytes().expect("écriture")) else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    assert!(bleus > 20_000, "l'image remplacée n'apparaît pas ({bleus})");
    assert!(rouges < 500, "l'ancienne image est encore là ({rouges})");
}

/// Image 8×8 en deux moitiés de couleur : la gauche rouge, la droite bleue.
fn moities_gauche_droite() -> Vec<u8> {
    let mut pixels = Vec::new();
    for row in 0..8 {
        for column in 0..8 {
            if column < 4 {
                pixels.extend_from_slice(&[220, 40, 40]);
            } else {
                pixels.extend_from_slice(&[40, 40, 220]);
            }
            let _ = row;
        }
    }
    pixels
}

/// Image 8×8 en deux moitiés de couleur : le haut rouge, le bas bleu.
fn moities_haut_bas() -> Vec<u8> {
    let mut pixels = Vec::new();
    for row in 0..8 {
        for _ in 0..8 {
            if row < 4 {
                pixels.extend_from_slice(&[220, 40, 40]);
            } else {
                pixels.extend_from_slice(&[40, 40, 220]);
            }
        }
    }
    pixels
}

/// Centre de gravité des pixels d'une couleur franche, en coordonnées bitmap.
///
/// Le rendu est la seule preuve que la bonne moitié de l'image est allée au
/// bon endroit.
fn centroide(bitmap: &PageBitmap, rouge: bool) -> Option<(f32, f32)> {
    let width = bitmap.width as usize;
    let mut sum_x = 0.0f64;
    let mut sum_y = 0.0f64;
    let mut count = 0.0f64;
    for (index, px) in bitmap.pixels.chunks_exact(4).enumerate() {
        let red = px[0] as i32 - px[2] as i32 > 60 && px[0] as i32 - px[1] as i32 > 60;
        let blue = px[2] as i32 - px[0] as i32 > 60 && px[2] as i32 - px[1] as i32 > 60;
        // Les deux tests sont explicites : « pas rouge » attraperait le fond
        // blanc et fausserait le centre de gravité.
        if (rouge && red) || (!rouge && blue) {
            sum_x += (index % width) as f64;
            sum_y += (index / width) as f64;
            count += 1.0;
        }
    }
    (count > 0.0).then(|| ((sum_x / count) as f32, (sum_y / count) as f32))
}

/// Un quart de tour horaire amène la moitié gauche de l'image en haut.
#[test]
fn rotation_horaire_amene_la_gauche_en_haut() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");

    let renderer = PdfiumRenderer::load().expect("pdfium");
    let avant = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (rouge_x, _) = centroide(&avant, true).expect("moitié rouge");
    let (bleu_x, _) = centroide(&avant, false).expect("moitié bleue");
    assert!(rouge_x < bleu_x, "au départ, le rouge est à gauche");

    doc.transform_image(&images[0], ImageTransform::RotateCw)
        .expect("rotation");
    let apres = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (rouge_x, rouge_y) = centroide(&apres, true).expect("moitié rouge");
    let (bleu_x, bleu_y) = centroide(&apres, false).expect("moitié bleue");
    assert!(
        rouge_y < bleu_y && (rouge_x - bleu_x).abs() < 30.0,
        "le rouge devrait être au-dessus du bleu : rouge=({rouge_x},{rouge_y}) bleu=({bleu_x},{bleu_y})"
    );
    // Les pixels ne sont pas réencodés : la ressource garde sa taille.
    let apres_infos = doc.extract_images(1).expect("liste des images");
    assert_eq!((apres_infos[0].width, apres_infos[0].height), (8, 8));
}

/// Un miroir vertical amène la moitié haute en bas.
#[test]
fn miroir_vertical_amene_le_haut_en_bas() {
    let mut doc = document_avec_image(&moities_haut_bas(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");

    let renderer = PdfiumRenderer::load().expect("pdfium");
    let avant = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (_, rouge_y) = centroide(&avant, true).expect("moitié rouge");
    let (_, bleu_y) = centroide(&avant, false).expect("moitié bleue");
    assert!(rouge_y < bleu_y, "au départ, le rouge est en haut");

    doc.transform_image(&images[0], ImageTransform::FlipVertical)
        .expect("miroir");
    let apres = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (_, rouge_y) = centroide(&apres, true).expect("moitié rouge");
    let (_, bleu_y) = centroide(&apres, false).expect("moitié bleue");
    assert!(
        rouge_y > bleu_y,
        "après le miroir vertical, le rouge devrait être en bas : rouge_y={rouge_y} bleu_y={bleu_y}"
    );
}

/// Le quart de tour antihoraire amène la moitié gauche en bas.
#[test]
fn rotation_antihoraire_amene_la_gauche_en_bas() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");

    let renderer = PdfiumRenderer::load().expect("pdfium");
    doc.transform_image(&images[0], ImageTransform::RotateCcw)
        .expect("rotation");
    let apres = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (rouge_x, rouge_y) = centroide(&apres, true).expect("moitié rouge");
    let (bleu_x, bleu_y) = centroide(&apres, false).expect("moitié bleue");
    assert!(
        rouge_y > bleu_y && (rouge_x - bleu_x).abs() < 30.0,
        "le rouge devrait être sous le bleu : rouge=({rouge_x},{rouge_y}) bleu=({bleu_x},{bleu_y})"
    );
}

/// Le miroir horizontal amène la moitié gauche à droite.
#[test]
fn miroir_horizontal_amene_la_gauche_a_droite() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");

    let renderer = PdfiumRenderer::load().expect("pdfium");
    doc.transform_image(&images[0], ImageTransform::FlipHorizontal)
        .expect("miroir");
    let apres = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (rouge_x, _) = centroide(&apres, true).expect("moitié rouge");
    let (bleu_x, _) = centroide(&apres, false).expect("moitié bleue");
    assert!(
        rouge_x > bleu_x,
        "après le miroir horizontal, le rouge devrait être à droite : rouge_x={rouge_x} bleu_x={bleu_x}"
    );
}

/// Un rognage après une rotation découpe l'image selon **son** axe, pas celui de
/// l'écran : rogner le bord droit de l'image retire ce qui s'affiche en bas.
#[test]
fn rognage_apres_rotation_suit_l_axe_de_l_image() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");
    doc.transform_image(&images[0], ImageTransform::RotateCw)
        .expect("rotation");

    // Après le quart de tour, la moitié gauche (rouge) s'affiche en haut.
    let renderer = PdfiumRenderer::load().expect("pdfium");
    let avant = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let (_, rouge_y) = centroide(&avant, true).expect("moitié rouge");
    let (_, bleu_y) = centroide(&avant, false).expect("moitié bleue");
    assert!(rouge_y < bleu_y, "le rouge devrait être en haut après rotation");

    doc.crop_image(
        &images[0],
        ImageCrop {
            right: 0.5,
            ..ImageCrop::default()
        },
    )
    .expect("rognage");

    let apres = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .expect("rendu");
    let bleus = apres
        .pixels
        .chunks_exact(4)
        .filter(|px| px[2] as i32 - px[0] as i32 > 60 && px[2] as i32 - px[1] as i32 > 60)
        .count();
    let rouges = apres
        .pixels
        .chunks_exact(4)
        .filter(|px| px[0] as i32 - px[2] as i32 > 60 && px[0] as i32 - px[1] as i32 > 60)
        .count();
    assert_eq!(
        bleus, 0,
        "la moitié droite de l'image (bleue, affichée en bas) aurait dû être découpée"
    );
    assert!(rouges > 0, "la moitié conservée a disparu");
    // L'image découpée garde sa place : la boîte reste la moitié visible.
    let infos = doc.extract_images(1).expect("liste des images");
    assert_eq!((infos[0].width, infos[0].height), (4, 8));
}

/// Supprimer une image laisse le flux équilibré : autant de `q` que de `Q`.
#[test]
fn suppression_laisse_le_flux_equilibre() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");
    doc.delete_image(&images[0]).expect("suppression");

    let bytes = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // Seuls les flux de contenu comptent : on lit entre les mots-clés.
    assert_eq!(
        text.matches("\nq\n").count(),
        text.matches("\nQ\n").count(),
        "le flux de contenu n'est plus équilibré"
    );
}

/// Une rotation n'abîme pas la boîte : elle reste au même endroit, seuls les
/// vecteurs tournent.
#[test]
fn rotation_garde_la_boite() {
    let mut doc = document_avec_image(&moities_gauche_droite(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");
    let avant = doc.image_rect(&images[0]).expect("boîte").expect("une boîte");

    doc.transform_image(&images[0], ImageTransform::RotateCcw)
        .expect("rotation");
    let apres = doc.image_rect(&images[0]).expect("boîte").expect("une boîte");
    for (a, b) in avant.iter().zip(apres.iter()) {
        assert!((a - b).abs() < 0.5, "la boîte a bougé : {avant:?} → {apres:?}");
    }
}

/// Rognage : les pixels sont découpés et la boîte de dessin rétrécit.
#[test]
fn rognage_decoupe_les_pixels_et_la_boite() {
    let mut carre = image::RgbImage::new(8, 8);
    for px in carre.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let mut doc = document_avec_image(&carre.into_raw(), 8, 8);

    let images = doc.extract_images(1).expect("liste des images");
    let avant = premiere_matrice(&doc, &images[0].resource_name).expect("matrice initiale");

    doc.crop_image(
        &images[0],
        ImageCrop {
            left: 0.25,
            right: 0.25,
            ..ImageCrop::default()
        },
    )
    .expect("rognage");

    let apres = doc.extract_images(1).expect("liste des images");
    assert_eq!(
        (apres[0].width, apres[0].height),
        (4, 8),
        "les pixels devraient être découpés de moitié"
    );
    let matrice = premiere_matrice(&doc, &apres[0].resource_name).expect("matrice finale");
    assert!(
        (matrice[0] - avant[0] * 0.5).abs() < 0.01,
        "le vecteur x de la boîte devrait être réduit de moitié : {avant:?} → {matrice:?}"
    );
    assert!(
        (matrice[4] - (avant[4] + avant[0] * 0.25)).abs() < 0.01,
        "l'origine devrait glisser du quart rogné : {avant:?} → {matrice:?}"
    );
}

/// Matrice `cm` qui précède le dessin d'une image, relue dans le fichier.
fn premiere_matrice(doc: &Document, resource_name: &str) -> Option<[f32; 6]> {
    let bytes = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let draw = format!("/{} Do", resource_name.trim_start_matches('/'));
    let position = text.find(&draw)?;
    let before = &text[..position];
    let cm = before.rfind(" cm\n")?;
    let line_start = before[..cm].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let numbers: Vec<f32> = before[line_start..cm]
        .split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect();
    let tail: Vec<f32> = numbers.iter().rev().take(6).rev().copied().collect();
    tail.try_into().ok()
}

/// L'opacité passe par un état graphique posé avant le dessin.
#[test]
fn opacite_pose_un_etat_graphique() {
    let mut carre = image::RgbImage::new(8, 8);
    for px in carre.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let mut doc = document_avec_image(&carre.into_raw(), 8, 8);

    let images = doc.extract_images(1).expect("liste des images");
    doc.set_image_opacity(&images[0], 0.4).expect("opacité");

    let bytes = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/ca 0.4"), "l'alpha de remplissage manque");
    assert!(text.contains("/CA 0.4"), "l'alpha de trait manque");
    assert!(text.contains(" gs\n"), "le `/gs` n'est pas dans le flux");
    // Le `/gs` doit précéder le `Do` de l'image, sinon il ne s'applique pas.
    let draw = text.find(" Do\n").expect("un dessin");
    let gs = text.find(" gs\n").expect("un état graphique");
    assert!(gs < draw, "l'état graphique est posé après le dessin");
}

/// Supprimer une image retire son dessin et sa ressource.
#[test]
fn supprimer_une_image_retire_le_dessin() {
    let mut carre = image::RgbImage::new(8, 8);
    for px in carre.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let mut doc = document_avec_image(&carre.into_raw(), 8, 8);

    let images = doc.extract_images(1).expect("liste des images");
    doc.delete_image(&images[0]).expect("suppression");

    assert!(
        doc.extract_images(1).expect("liste des images").is_empty(),
        "la ressource devrait avoir disparu"
    );
    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert!(!text.contains(" Do\n"), "le dessin est encore dans le flux");

    if let Some((rouges, _)) = rendre_couleurs(&doc.to_bytes().expect("écriture")) {
        assert_eq!(rouges, 0, "l'image supprimée est encore visible");
    }
}

/// Deux images posées sur la même page gardent chacune leur ressource et leur
/// boîte : sans cela, la seconde écraserait la première.
#[test]
fn deux_images_sur_une_page_restent_distinctes() {
    let mut rouge = image::RgbImage::new(8, 8);
    for px in rouge.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let raw = rouge.into_raw();

    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.insert_image_bitmap(1, [50.0, 500.0, 150.0, 600.0], &raw, 8, 8)
        .expect("première image");
    doc.insert_image_bitmap(1, [250.0, 300.0, 350.0, 400.0], &raw, 8, 8)
        .expect("seconde image");

    let images = doc.extract_images(1).expect("liste des images");
    assert_eq!(images.len(), 2, "deux images attendues");
    assert_ne!(
        images[0].resource_name, images[1].resource_name,
        "les deux images partagent le même nom de ressource"
    );

    let mut boites: Vec<[f32; 4]> = images
        .iter()
        .map(|info| doc.image_rect(info).expect("boîte").expect("une boîte"))
        .collect();
    boites.sort_by(|a, b| a[1].partial_cmp(&b[1]).expect("ordre"));
    assert!(
        (boites[0][0] - 250.0).abs() < 1.0 && (boites[1][0] - 50.0).abs() < 1.0,
        "les boîtes rendues ne correspondent pas aux positions : {boites:?}"
    );
    assert!((boites[0][3] - 400.0).abs() < 1.0, "hauteur de la première boîte");
}

/// Régler l'opacité deux fois ne pose qu'un état graphique : le préambule du
/// dessin est remplacé, pas empilé.
#[test]
fn opacite_reglee_deux_fois_ne_s_empile_pas() {
    let mut carre = image::RgbImage::new(8, 8);
    for px in carre.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    let mut doc = document_avec_image(&carre.into_raw(), 8, 8);
    let images = doc.extract_images(1).expect("liste des images");

    doc.set_image_opacity(&images[0], 0.8).expect("première opacité");
    doc.set_image_opacity(&images[0], 0.3).expect("seconde opacité");

    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert_eq!(text.matches(" gs\n").count(), 1, "un seul `/gs` attendu");
    assert!(text.contains("/ca 0.3"), "l'opacité n'a pas été remplacée");
}
