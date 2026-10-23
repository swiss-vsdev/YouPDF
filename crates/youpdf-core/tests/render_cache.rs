//! Cache du document pdfium : il doit survivre aux appels répétés et être
//! invalidé par toute modification du document.
//!
//! Le cache vit dans `PdfiumRenderer` et sa clé est le couple (identité du
//! document, génération). Deux erreurs sont fatales et testées ici :
//! reparsé à chaque page (lent), ou jamais invalidé (rendu périmé).

use youpdf_core::{
    Annotation, AnnotationKind, Document, PageBitmap, PdfiumRenderer, RenderOptions, Renderer,
};

/// Une page 612×792 avec un peu de texte.
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
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Un surlignage jaune au milieu de la page.
fn surlignage() -> Annotation {
    Annotation::new(1, AnnotationKind::Highlight, [100.0, 400.0, 400.0, 440.0])
}

/// Pixels franchement jaunes : la trace du surlignage.
fn pixels_jaunes(bitmap: &PageBitmap) -> usize {
    bitmap
        .pixels
        .chunks_exact(4)
        .filter(|px| px[0] as i32 - px[2] as i32 > 40 && px[1] as i32 - px[2] as i32 > 30)
        .count()
}

fn rendre(renderer: &PdfiumRenderer, doc: &Document) -> PageBitmap {
    renderer
        .render_page(doc, 1, &RenderOptions::default())
        .expect("rendu")
}

/// Le rendu doit rester correct quand le même renderer sert plusieurs pages
/// et plusieurs documents : le cache ne doit pas mélanger deux documents.
#[test]
fn le_cache_ne_melange_pas_deux_documents() {
    let Ok(renderer) = PdfiumRenderer::load() else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };

    let mut annote = Document::from_bytes(&page_blanche()).expect("ouverture");
    annote
        .apply_annotations(&[surlignage()])
        .expect("annotation");
    let vierge = Document::from_bytes(&page_blanche()).expect("ouverture");

    // Tous deux à la génération 0 : seule l'identité d'instance les distingue.
    let rendu_annote = rendre(&renderer, &annote);
    let rendu_vierge = rendre(&renderer, &vierge);
    assert!(pixels_jaunes(&rendu_annote) > 0, "annotation non rendue");
    assert_eq!(
        pixels_jaunes(&rendu_vierge),
        0,
        "le renderer a resservi les pages du document précédent"
    );
}

/// Le cœur de la correction : modifier le document puis rendre à nouveau
/// doit afficher la modification, et non la version mémorisée.
#[test]
fn une_retouche_est_visible_au_rendu_suivant() {
    let Ok(renderer) = PdfiumRenderer::load() else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");

    let avant = rendre(&renderer, &doc);
    assert_eq!(pixels_jaunes(&avant), 0, "page déjà annotée ?");

    let generation = doc.generation();
    doc.apply_annotations(&[surlignage()]).expect("annotation");
    assert!(
        doc.generation() > generation,
        "une mutation doit incrémenter la génération"
    );

    let apres = rendre(&renderer, &doc);
    assert!(
        pixels_jaunes(&apres) > 0,
        "rendu périmé : l'annotation ajoutée n'apparaît pas"
    );

    // Retirer l'annotation doit aussi se voir (invalidation dans les deux sens).
    doc.apply_annotations(&[]).expect("nettoyage");
    let apres_nettoyage = rendre(&renderer, &doc);
    assert_eq!(
        pixels_jaunes(&apres_nettoyage),
        0,
        "rendu périmé : l'annotation retirée est encore peinte"
    );
}

/// Deux renderers distincts doivent pouvoir coexister et rendre : pdfium
/// n'est initialisé qu'une fois par processus (deux initialisations
/// bloquaient indéfiniment).
#[test]
fn deux_renderers_coexistent() {
    let Ok(premier) = PdfiumRenderer::load() else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    let second = PdfiumRenderer::load().expect("deuxième renderer");
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");

    assert!(rendre(&premier, &doc).width > 0);
    assert!(rendre(&second, &doc).width > 0);
}
