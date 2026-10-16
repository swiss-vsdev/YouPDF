//! Création de documents : page vierge, PDF à partir d'images, fusion.
//!
//! Chaque test échoue si la construction ou la fusion casse : comptage et
//! taille des pages, réouverture depuis `to_bytes()`, ordre des pages après
//! insertion.

use youpdf_core::{blank_document, document_from_images, Document};

/// Comparaison de tailles flottantes (les points sont stockés en `f32`).
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

fn assert_size(doc: &Document, page: u32, width: f32, height: f32) {
    let (w, h) = doc.page_size(page);
    assert!(
        approx(w, width) && approx(h, height),
        "page {page} : taille {w}×{h}, attendue {width}×{height}"
    );
}

#[test]
fn blank_document_a_bon_taille_et_se_reouvre() {
    let doc = blank_document(3, 595.0, 842.0).expect("création");
    assert_eq!(doc.page_count(), 3, "trois pages");
    for page in 1..=3 {
        assert_size(&doc, page, 595.0, 842.0);
    }

    // La sérialisation doit être relisible telle quelle.
    let bytes = doc.to_bytes().expect("sérialisation");
    let reopened = Document::from_bytes(&bytes).expect("réouverture");
    assert_eq!(reopened.page_count(), 3);
    assert_size(&reopened, 1, 595.0, 842.0);

    // Au moins une page est exigée.
    assert!(blank_document(0, 595.0, 842.0).is_err(), "zéro page refusé");
}

#[test]
fn document_from_images_une_page_par_image() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let path = dir.path().join("rouge.png");

    // PNG minuscule, entièrement rouge.
    let mut img = image::RgbImage::new(8, 8);
    for px in img.pixels_mut() {
        *px = image::Rgb([220, 40, 40]);
    }
    img.save(&path).expect("écriture du PNG");

    // 8 px à 96 dpi = 8 / 96 * 72 = 6 points.
    let doc = document_from_images(&[path], 96.0).expect("création");
    assert_eq!(doc.page_count(), 1, "une page par image");
    assert_size(&doc, 1, 6.0, 6.0);

    // Les octets produits doivent être relisibles.
    let bytes = doc.to_bytes().expect("sérialisation");
    let reopened = Document::from_bytes(&bytes).expect("réouverture");
    assert_eq!(reopened.page_count(), 1);
    assert_size(&reopened, 1, 6.0, 6.0);

    // Aucune image : entrée invalide.
    assert!(document_from_images(&[], 96.0).is_err(), "liste vide refusée");
}

/// Séquence des tailles de page, pour vérifier l'ordre après insertion.
fn sizes(doc: &Document) -> Vec<(f32, f32)> {
    (1..=doc.page_count() as u32)
        .map(|page| doc.page_size(page))
        .collect()
}

fn assert_sizes(doc: &Document, expected: &[(f32, f32)]) {
    let actual = sizes(doc);
    assert_eq!(actual.len(), expected.len(), "nombre de pages");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            approx(a.0, e.0) && approx(a.1, e.1),
            "page {} : {a:?}, attendue {e:?}",
            i + 1
        );
    }
}

#[test]
fn insert_document_tete_milieu_fin() {
    let source = blank_document(2, 200.0, 200.0).expect("source");

    // En tête.
    let mut front = blank_document(2, 100.0, 100.0).expect("base");
    let inserted = front.insert_document(0, &source).expect("insertion en tête");
    assert_eq!(inserted, 2);
    assert_sizes(
        &front,
        &[(200.0, 200.0), (200.0, 200.0), (100.0, 100.0), (100.0, 100.0)],
    );

    // Au milieu : après la page 1.
    let mut middle = blank_document(2, 100.0, 100.0).expect("base");
    middle.insert_document(1, &source).expect("insertion au milieu");
    assert_sizes(
        &middle,
        &[(100.0, 100.0), (200.0, 200.0), (200.0, 200.0), (100.0, 100.0)],
    );

    // Après la dernière : ajout à la fin.
    let mut end = blank_document(2, 100.0, 100.0).expect("base");
    end.insert_document(99, &source).expect("insertion en fin");
    assert_sizes(
        &end,
        &[(100.0, 100.0), (100.0, 100.0), (200.0, 200.0), (200.0, 200.0)],
    );

    // L'insertion doit survivre à une sérialisation.
    let bytes = middle.to_bytes().expect("sérialisation");
    let reopened = Document::from_bytes(&bytes).expect("réouverture");
    assert_sizes(
        &reopened,
        &[(100.0, 100.0), (200.0, 200.0), (200.0, 200.0), (100.0, 100.0)],
    );
}

#[test]
fn insert_document_sur_sa_propre_copie_ne_panique_pas() {
    let mut doc = blank_document(2, 100.0, 100.0).expect("base");
    // Réplique du même document : la source est sérialisée avant mutation,
    // donc l'insertion ne corrompt pas la source.
    let bytes = doc.to_bytes().expect("sérialisation");
    let copy = Document::from_bytes(&bytes).expect("copie");

    let inserted = doc.insert_document(1, &copy).expect("insertion");
    assert_eq!(inserted, 2);
    assert_eq!(doc.page_count(), 4, "les pages ont doublé");
    assert_sizes(
        &doc,
        &[(100.0, 100.0), (100.0, 100.0), (100.0, 100.0), (100.0, 100.0)],
    );
}
