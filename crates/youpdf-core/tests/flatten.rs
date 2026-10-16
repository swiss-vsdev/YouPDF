//! Apparences d'annotation et aplatissement.
//!
//! Une annotation PDF sans `/AP` n'est qu'un rectangle décrit : chaque
//! lecteur dessine ce qu'il veut — souvent rien. Ces tests vérifient que
//! YouPDF pose une apparence exploitable, que PDFium la rend, et que
//! l'aplatissement donne exactement le même dessin, cette fois dans le
//! contenu de la page.

use youpdf_core::{
    Annotation, AnnotationKind, Document, PageBitmap, PdfiumRenderer, RenderOptions, Renderer,
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

/// Un surlignage jaune au milieu d'une zone blanche.
fn surlignage() -> Annotation {
    let mut annot = Annotation::new(1, AnnotationKind::Highlight, [100.0, 400.0, 400.0, 440.0]);
    annot.content = "surligné".to_string();
    annot
}

/// Dictionnaire de la première annotation du document.
fn annot_dict(bytes: &[u8]) -> lopdf::Dictionary {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    for (_num, id) in doc.get_pages() {
        let page = doc.get_dictionary(id).expect("page");
        let Ok(annots) = page.get(b"Annots") else {
            continue;
        };
        for annot in annots.as_array().expect("tableau /Annots") {
            if let Ok(id) = annot.as_reference() {
                return doc.get_dictionary(id).expect("annotation").clone();
            }
        }
    }
    panic!("aucune annotation dans le document");
}

/// Contenu de l'apparence `/AP /N` de la première annotation.
fn appearance_content(bytes: &[u8]) -> Vec<u8> {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    for (_num, id) in doc.get_pages() {
        let page = doc.get_dictionary(id).expect("page");
        let Ok(annots) = page.get(b"Annots") else {
            continue;
        };
        for annot in annots.as_array().expect("tableau /Annots") {
            let Ok(id) = annot.as_reference() else {
                continue;
            };
            let dict = doc.get_dictionary(id).expect("annotation");
            let Ok(ap) = dict.get(b"AP").and_then(|o| o.as_dict()) else {
                continue;
            };
            let n = ap.get(b"N").expect("/AP /N");
            let stream = match n {
                lopdf::Object::Reference(id) => {
                    doc.get_object(*id).expect("flux").as_stream().expect("flux")
                }
                lopdf::Object::Stream(s) => s,
                other => panic!("/AP /N inattendu : {other:?}"),
            };
            return stream.content.clone();
        }
    }
    panic!("aucune apparence");
}

/// Pixels franchement jaunes : c'est la trace du surlignage.
fn pixels_jaunes(bitmap: &PageBitmap) -> usize {
    bitmap
        .pixels
        .chunks_exact(4)
        .filter(|px| px[0] as i32 - px[2] as i32 > 40 && px[1] as i32 - px[2] as i32 > 30)
        .count()
}

/// Rend la page 1 et renvoie le nombre de pixels jaunes.
fn rendre(bytes: &[u8]) -> Option<usize> {
    let renderer = PdfiumRenderer::load().ok()?;
    let doc = Document::from_bytes(bytes).ok()?;
    let bitmap = renderer
        .render_page(&doc, 1, &RenderOptions::default())
        .ok()?;
    Some(pixels_jaunes(&bitmap))
}

/// L'annotation doit porter une apparence : c'est elle qui la fait exister
/// hors de YouPDF (autre lecteur, impression, conversion).
#[test]
fn annotation_pose_une_apparence() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.apply_annotations(&[surlignage()]).expect("annotation");

    let bytes = doc.to_bytes().expect("écriture");

    let dict = annot_dict(&bytes);
    assert!(dict.get(b"AP").is_ok(), "pas d'apparence /AP");
    let content = appearance_content(&bytes);
    let text = String::from_utf8_lossy(&content);
    assert!(
        text.contains(" re f"),
        "l'apparence d'un surlignage doit remplir son rectangle : {text:?}"
    );
    assert!(text.contains("gs"), "transparence attendue : {text:?}");
}

/// PDFium doit réellement peindre cette apparence.
#[test]
fn l_apparence_se_voit_au_rendu() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.apply_annotations(&[surlignage()]).expect("annotation");
    let bytes = doc.to_bytes().expect("écriture");

    let Some(jaunes) = rendre(&bytes) else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    assert!(
        jaunes > 500,
        "surlignage invisible au rendu ({jaunes} pixels)"
    );
}

/// Aplatir doit donner le même dessin, sans annotation.
#[test]
fn aplatir_conserve_le_dessin() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.apply_annotations(&[surlignage()]).expect("annotation");
    let avant = doc.to_bytes().expect("écriture");

    let flattened = doc.flatten_annotations().expect("aplatissement");
    assert_eq!(flattened, 1, "une annotation devait être aplatie");
    let apres = doc.to_bytes().expect("écriture");

    // Plus d'annotation, mais le contenu de la page porte le dessin.
    let reloaded = lopdf::Document::load_mem(&apres).expect("relecture");
    for (_num, id) in reloaded.get_pages() {
        let page = reloaded.get_dictionary(id).expect("page");
        assert!(
            page.get(b"Annots").is_err(),
            "l'annotation devait disparaître de /Annots"
        );
    }
    let content = page_content_of(&apres);
    let text = String::from_utf8_lossy(&content);
    assert!(text.contains(" Do"), "apparence non peinte : {text:?}");
    assert!(text.contains(" cm"), "matrice de placement absente : {text:?}");

    if let (Some(avant), Some(apres)) = (rendre(&avant), rendre(&apres)) {
        let ecart = (avant as f32 - apres as f32).abs() / avant.max(1) as f32;
        assert!(
            ecart < 0.2,
            "le dessin aplati diffère du dessin d'annotation : {avant} → {apres} pixels"
        );
    }
}

/// L'enregistrement pose les annotations dans le fichier écrit, et les
/// laisse hors du document en mémoire : l'interface les dessine elle-même,
/// l'onglet les garde, le fichier les contient.
#[test]
fn enregistrer_pose_les_annotations_sans_salir_l_etat() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let annots = vec![surlignage()];

    let path = std::env::temp_dir().join(format!("youpdf-save-{}.pdf", std::process::id()));
    doc.save_with_annotations(&path, &annots).expect("écriture");
    let bytes = std::fs::read(&path).expect("lecture du fichier écrit");
    let _ = std::fs::remove_file(&path);

    // Le fichier porte l'annotation, son apparence, et se relit.
    let dict = annot_dict(&bytes);
    assert!(dict.get(b"AP").is_ok(), "apparence absente du fichier écrit");
    let relues = Document::from_bytes(&bytes)
        .expect("relecture")
        .read_annotations();
    assert_eq!(relues.len(), 1, "l'annotation doit se relire");

    // En mémoire, le document reste sans annotation : l'onglet les garde.
    assert!(
        doc.read_annotations().is_empty(),
        "le document en mémoire ne doit pas porter les annotations d'édition"
    );
    assert_eq!(doc.page_count(), 1);
}

/// Une annotation sans apparence (un lien) reste en place : l'aplatir
/// effacerait une information que le lecteur seul sait présenter.
#[test]
fn aplatir_laisse_les_liens() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let mut lien = Annotation::new(1, AnnotationKind::Link, [60.0, 690.0, 200.0, 730.0]);
    lien.content = "https://example.org".to_string();
    doc.apply_annotations(&[lien]).expect("annotation");

    let flattened = doc.flatten_annotations().expect("aplatissement");
    assert_eq!(flattened, 0, "un lien n'a rien à aplatir");

    let bytes = doc.to_bytes().expect("écriture");
    let dict = annot_dict(&bytes);
    assert_eq!(
        dict.get(b"Subtype").and_then(|o| o.as_name_str()).ok(),
        Some("Link"),
        "le lien devait rester dans /Annots"
    );
}

/// Contenu concaténé de la page 1.
fn page_content_of(bytes: &[u8]) -> Vec<u8> {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    let mut out = Vec::new();
    for (_num, id) in doc.get_pages() {
        let page = doc.get_dictionary(id).expect("page");
        let Ok(contents) = page.get(b"Contents") else {
            continue;
        };
        match contents {
            lopdf::Object::Reference(id) => {
                if let Ok(stream) = doc.get_object(*id).and_then(|o| o.as_stream()) {
                    out.extend_from_slice(&stream.content);
                }
            }
            lopdf::Object::Array(arr) => {
                for item in arr {
                    if let Ok(id) = item.as_reference() {
                        if let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) {
                            out.extend_from_slice(&stream.content);
                        }
                    }
                }
            }
            _ => {}
        }
        break;
    }
    out
}
