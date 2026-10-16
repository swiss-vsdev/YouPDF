//! Extraction de tableaux et de paires clé/valeur.
//!
//! Les documents de test sont construits en mémoire : chaque cellule est un
//! fragment positionné par un opérateur `Tm`, dans son propre bloc `BT…ET`,
//! pour que l'extracteur de fragments ne les fusionne pas.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::Document;

/// Un fragment de texte positionné en absolu : `x y` = ligne de base.
fn frag(x: f32, y: f32, size: f32, text: &str) -> String {
    format!("BT /F1 {size} Tf 1 0 0 1 {x} {y} Tm ({text}) Tj ET\n")
}

/// Construit un PDF d'une page dont le flux de contenu est `content`.
fn pdf_with(content: &str) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let stream = doc.add_object(Stream::new(
        dictionary! {},
        content.as_bytes().to_vec(),
    ));
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

/// Grille 3 × 3 : trois lignes à trois colonnes alignées.
fn grille_trois_trois() -> Vec<u8> {
    let rows = [
        ["A1", "B1", "C1"],
        ["A2", "B2", "C2"],
        ["A3", "B3", "C3"],
    ];
    let mut content = String::new();
    for (r, row) in rows.iter().enumerate() {
        let y = 700.0 - r as f32 * 20.0;
        for (c, text) in row.iter().enumerate() {
            let x = 100.0 + c as f32 * 100.0;
            content.push_str(&frag(x, y, 11.0, text));
        }
    }
    pdf_with(&content)
}

/// Un tableau 3 × 3 est détecté, avec le bon texte de cellule.
#[test]
fn tableau_trois_trois() {
    let doc = Document::from_bytes(&grille_trois_trois()).expect("ouverture");
    let tables = doc.extract_tables().expect("extraction");

    assert_eq!(tables.len(), 1, "un seul tableau attendu");
    assert_eq!(tables[0].page, 1);
    assert_eq!(
        tables[0].rows,
        vec![
            vec!["A1".to_string(), "B1".to_string(), "C1".to_string()],
            vec!["A2".to_string(), "B2".to_string(), "C2".to_string()],
            vec!["A3".to_string(), "B3".to_string(), "C3".to_string()],
        ]
    );
}

/// Deux colonnes de paires clé/valeur sont détectées, en ordre de lecture.
#[test]
fn paires_cle_valeur_deux_colonnes() {
    let mut content = String::new();

    // Ligne 1 : deux paires côte à côte.
    content.push_str(&frag(60.0, 700.0, 11.0, "Nom :"));
    content.push_str(&frag(180.0, 700.0, 11.0, "Dupont"));
    content.push_str(&frag(320.0, 700.0, 11.0, "Ville :"));
    content.push_str(&frag(440.0, 700.0, 11.0, "Paris"));

    // Ligne 2 : idem.
    content.push_str(&frag(60.0, 680.0, 11.0, "Age :"));
    content.push_str(&frag(180.0, 680.0, 11.0, "42"));
    content.push_str(&frag(320.0, 680.0, 11.0, "Pays :"));
    content.push_str(&frag(440.0, 680.0, 11.0, "France"));

    let doc = Document::from_bytes(&pdf_with(&content)).expect("ouverture");
    let pairs = doc.extract_key_values().expect("extraction");

    let observed: Vec<(String, String)> = pairs
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.clone()))
        .collect();

    assert_eq!(
        observed,
        vec![
            ("Nom".to_string(), "Dupont".to_string()),
            ("Ville".to_string(), "Paris".to_string()),
            ("Age".to_string(), "42".to_string()),
            ("Pays".to_string(), "France".to_string()),
        ]
    );
    assert!(pairs.iter().all(|kv| kv.page == 1));
}

/// Un paragraphe sans alignement ne produit aucun tableau.
#[test]
fn paragraphe_sans_alignement_pas_de_tableau() {
    let mut content = String::new();
    for (i, line) in [
        "Ceci est un paragraphe",
        "avec des lignes de texte",
        "dont les debuts ne sont pas",
        "alignes verticalement.",
    ]
    .iter()
    .enumerate()
    {
        // Ordonnées et abscisses volontairement irrégulières.
        let y = 700.0 - i as f32 * 18.0;
        let x = 72.0 + (i as f32) * 3.0;
        content.push_str(&frag(x, y, 11.0, line));
    }

    let doc = Document::from_bytes(&pdf_with(&content)).expect("ouverture");
    let tables = doc.extract_tables().expect("extraction");
    assert!(tables.is_empty(), "aucun tableau attendu : {tables:?}");
}

/// Une page sans texte renvoie des résultats vides, sans paniquer.
#[test]
fn page_sans_texte_resultats_vides() {
    let doc = Document::from_bytes(&pdf_with("")).expect("ouverture");

    assert!(doc.extract_tables().expect("tableaux").is_empty());
    assert!(doc.extract_key_values().expect("paires").is_empty());
}
