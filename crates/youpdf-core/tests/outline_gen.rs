//! Génération automatique du plan (signets) à partir du texte.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::structure::{decode_pdf_text, OutlineStyle};
use youpdf_core::Document;

/// Construit un PDF dont chaque page porte des lignes de texte positionnées.
///
/// Chaque ligne est `(texte, taille de police, x, y)`.
fn text_pdf(pages: &[Vec<(&str, f32, f32, f32)>]) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for lines in pages {
        let mut content: Vec<u8> = Vec::new();
        for (text, size, x, y) in lines {
            content.extend_from_slice(
                format!("BT\n/F1 {size} Tf\n1 0 0 1 {x} {y} Tm\n({text}) Tj\nET\n").as_bytes(),
            );
        }
        let stream = doc.add_object(Stream::new(dictionary! {}, content));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => dictionary! {
                        "Type" => Object::Name(b"Font".to_vec()),
                        "Subtype" => Object::Name(b"Type1".to_vec()),
                        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
                    }
                }
            },
            "Contents" => stream,
        });
        kids.push(page.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => pages.len() as i64,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture du PDF de test");
    buf
}

#[test]
fn generate_produces_expected_count_and_nesting() {
    let pdf = text_pdf(&[
        vec![
            ("Chapter One", 24.0, 72.0, 720.0),
            ("Section A", 16.0, 72.0, 690.0),
            ("body one", 11.0, 72.0, 660.0),
            ("body two", 11.0, 72.0, 640.0),
        ],
        vec![
            ("Chapter Two", 24.0, 72.0, 720.0),
            ("Section B", 16.0, 72.0, 690.0),
            ("body three", 11.0, 72.0, 660.0),
            ("body four", 11.0, 72.0, 640.0),
        ],
    ]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let written = doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(written, 4, "deux chapitres et deux sections");

    let outline = doc.outline();
    assert_eq!(outline.len(), 4);
    let find = |title: &str| outline.iter().find(|item| item.title == title);
    assert_eq!(find("Chapter One").map(|item| item.level), Some(0));
    assert_eq!(find("Chapter Two").map(|item| item.level), Some(0));
    assert_eq!(find("Section A").map(|item| item.level), Some(1));
    assert_eq!(find("Section B").map(|item| item.level), Some(1));
    assert_eq!(find("Chapter One").and_then(|item| item.page), Some(1));
    assert_eq!(find("Chapter Two").and_then(|item| item.page), Some(2));
}

#[test]
fn generated_top_level_order_follows_the_document() {
    let pdf = text_pdf(&[
        vec![("First", 24.0, 72.0, 720.0), ("body", 11.0, 72.0, 700.0)],
        vec![("Second", 24.0, 72.0, 720.0), ("body", 11.0, 72.0, 700.0)],
    ]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");

    let root = reopened
        .catalog()
        .expect("catalogue")
        .get(b"Outlines")
        .expect("Outlines")
        .as_reference()
        .expect("référence");
    let mut titles = Vec::new();
    let mut current = reopened
        .get_dictionary(root)
        .expect("racine")
        .get(b"First")
        .ok()
        .and_then(|o| o.as_reference().ok());
    while let Some(id) = current {
        let node = reopened.get_dictionary(id).expect("entrée");
        let title = node.get(b"Title").expect("titre");
        titles.push(decode_pdf_text(title.as_str().expect("octets")));
        current = node.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }
    assert_eq!(titles, vec!["First".to_owned(), "Second".to_owned()]);
}

#[test]
fn body_only_document_produces_no_bookmark() {
    let pdf = text_pdf(&[vec![
        ("just body", 11.0, 72.0, 700.0),
        ("more body", 11.0, 72.0, 680.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let written = doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(written, 0, "aucun titre au-dessus de la taille du corps");
    assert!(doc.outline().is_empty());
}

#[test]
fn clear_outline_empties_the_tree() {
    let pdf = text_pdf(&[vec![
        ("Heading", 24.0, 72.0, 720.0),
        ("body", 11.0, 72.0, 700.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    assert_eq!(doc.outline().len(), 1);

    let removed = doc.clear_outline().expect("suppression");
    assert_eq!(removed, 1, "une entrée retirée");
    assert!(doc.outline().is_empty(), "le plan doit être vide");

    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");
    assert!(
        reopened.catalog().expect("catalogue").get(b"Outlines").is_err(),
        "la clé /Outlines doit disparaître du catalogue"
    );
}

#[test]
fn max_entries_caps_the_number_of_bookmarks() {
    let pdf = text_pdf(&[vec![
        ("H1", 24.0, 72.0, 740.0),
        ("H2", 24.0, 72.0, 710.0),
        ("H3", 24.0, 72.0, 680.0),
        ("body one", 11.0, 72.0, 650.0),
        ("body two", 11.0, 72.0, 630.0),
        ("body three", 11.0, 72.0, 610.0),
        ("body four", 11.0, 72.0, 590.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    let style = OutlineStyle {
        max_entries: 2,
        ..OutlineStyle::default()
    };
    let written = doc.generate_outline(&style).expect("plan");
    assert_eq!(written, 2, "le plafond s'applique");
    assert_eq!(doc.outline().len(), 2);
}

#[test]
fn generated_title_decodes_back_to_the_heading_text() {
    let pdf = text_pdf(&[vec![
        ("Café Général", 24.0, 72.0, 720.0),
        ("body", 11.0, 72.0, 700.0),
        ("body again", 11.0, 72.0, 680.0),
    ]]);
    let mut doc = Document::from_bytes(&pdf).expect("ouverture");
    doc.generate_outline(&OutlineStyle::default()).expect("plan");
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].title, "Café Général");
}

/// Aller-retour arbre → plan → arbre : les titres, l'ordre et les niveaux
/// doivent survivre à une écriture complète (`write_outline`).
#[test]
fn ecriture_du_plan_conserve_la_hierarchie() {
    use youpdf_core::structure::{OutlineEntry, OutlineTarget};

    let mut doc =
        Document::from_bytes(&text_pdf(&[vec![], vec![], vec![]])).expect("ouverture");
    let tree = vec![
        OutlineEntry {
            title: "Chapitre 1".to_owned(),
            page: Some(1),
            target: OutlineTarget::None,
            children: vec![
                OutlineEntry {
                    title: "Section 1.1".to_owned(),
                    page: Some(2),
                    target: OutlineTarget::None,
                    children: Vec::new(),
                },
                OutlineEntry {
                    title: "Section 1.2".to_owned(),
                    page: None,
                    target: OutlineTarget::None,
                    children: Vec::new(),
                },
            ],
        },
        OutlineEntry {
            title: "Chapitre 2".to_owned(),
            page: Some(3),
            target: OutlineTarget::None,
            children: Vec::new(),
        },
    ];

    let written = doc.write_outline(&tree).expect("écriture du plan");
    assert_eq!(written, 4);

    let back = doc.outline_tree();
    // La cible d'origine est relue telle quelle ; les entrées écrites sans
    // cible portent maintenant la destination `/Fit` calculée.
    assert_eq!(back[0].title, "Chapitre 1");
    assert_eq!(back[0].page, Some(1));
    assert_eq!(back[0].children.len(), 2);
    assert_eq!(back[0].children[0].title, "Section 1.1");
    assert_eq!(back[0].children[0].page, Some(2));
    assert_eq!(back[0].children[1].page, None);
    assert!(matches!(
        back[0].children[0].target,
        youpdf_core::structure::OutlineTarget::Dest(_)
    ));
    assert_eq!(back[1].title, "Chapitre 2");
    assert_eq!(back[1].page, Some(3));

    // La forme à plat reste celle qu'affichent les autres vues ; `outline()`
    // visite les frères avant de descendre, `outline_tree()` garde le
    // parcours en profondeur.
    let flat = doc.outline();
    let levels: Vec<(String, usize)> = flat
        .iter()
        .map(|item| (item.title.clone(), item.level))
        .collect();
    assert_eq!(
        levels,
        vec![
            ("Chapitre 1".to_owned(), 0),
            ("Chapitre 2".to_owned(), 0),
            ("Section 1.1".to_owned(), 1),
            ("Section 1.2".to_owned(), 1),
        ]
    );

    // Un arbre vide retire le plan.
    doc.write_outline(&[]).expect("retrait du plan");
    assert!(doc.outline_tree().is_empty());
}

/// Une destination d'origine (ici nommée) survit à un renommage : éditer un
/// signet ne doit pas réécrire la cible des autres.
#[test]
fn renommer_un_signet_garde_les_destinations() {
    use youpdf_core::structure::{OutlineEntry, OutlineTarget};

    let mut doc = Document::from_bytes(&text_pdf(&[vec![], vec![]])).expect("ouverture");
    let named = OutlineTarget::Dest(Object::Name(b"Chapitre".to_vec()));
    let mut tree = vec![OutlineEntry {
        title: "Ancien nom".to_owned(),
        page: None,
        target: named.clone(),
        children: Vec::new(),
    }];
    doc.write_outline(&tree).expect("écriture");

    tree[0].title = "Nouveau nom".to_owned();
    doc.write_outline(&tree).expect("renommage");

    let back = doc.outline_tree();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].title, "Nouveau nom");
    assert_eq!(back[0].target, named, "la destination nommée est intacte");
}

/// Une réécriture ne laisse pas les anciens dictionnaires de signets derrière
/// elle : vingt renommages ne doivent pas laisser vingt plans morts.
#[test]
fn reecrire_le_plan_ne_laisse_pas_d_orphelins() {
    use youpdf_core::structure::{OutlineEntry, OutlineTarget};

    let mut doc = Document::from_bytes(&text_pdf(&[vec![], vec![]])).expect("ouverture");
    let mut tree = vec![
        OutlineEntry {
            title: "Un".to_owned(),
            page: Some(1),
            target: OutlineTarget::None,
            children: Vec::new(),
        },
        OutlineEntry {
            title: "Deux".to_owned(),
            page: Some(2),
            target: OutlineTarget::None,
            children: Vec::new(),
        },
    ];

    for round in 0..20 {
        tree[0].title = format!("Un ({round})");
        doc.write_outline(&tree).expect("réécriture");
    }

    let bytes = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&bytes).expect("réouverture");
    let titled = reopened
        .objects
        .values()
        .filter(|object| {
            matches!(object, Object::Dictionary(dict) if dict.get(b"Title").is_ok())
        })
        .count();
    assert_eq!(titled, 2, "un seul plan doit subsister dans le fichier");
    assert_eq!(doc.outline_tree().len(), 2);

    // L'effacement du plan libère aussi ses dictionnaires.
    doc.clear_outline().expect("effacement du plan");
    let bytes = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&bytes).expect("réouverture");
    let titled = reopened
        .objects
        .values()
        .filter(|object| {
            matches!(object, Object::Dictionary(dict) if dict.get(b"Title").is_ok())
        })
        .count();
    assert_eq!(titled, 0, "plus aucun signet ne doit rester");
}

/// Une entrée qui vise par une action `/GoTo` (et non par `/Dest`) est relue
/// avec la même action, et sa page reste résolue.
#[test]
fn action_goto_traverse_une_reecriture() {
    use youpdf_core::structure::{OutlineEntry, OutlineTarget};

    let mut doc = Document::from_bytes(&text_pdf(&[vec![], vec![]])).expect("ouverture");
    let page_id = *doc.page_ids().get(&1).expect("page 1");
    let action = Object::Dictionary(lopdf::dictionary! {
        "S" => Object::Name(b"GoTo".to_vec()),
        "D" => Object::Array(vec![
            Object::Reference(page_id),
            Object::Name(b"Fit".to_vec()),
        ]),
    });
    let tree = vec![OutlineEntry {
        title: "Aller".to_owned(),
        page: Some(1),
        target: OutlineTarget::Action(action.clone()),
        children: Vec::new(),
    }];
    doc.write_outline(&tree).expect("écriture");

    let back = doc.outline_tree();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].page, Some(1), "la page reste résolue par `/A /D`");
    assert_eq!(back[0].target, OutlineTarget::Action(action));
}

/// Une destination qui vise une page absente (supprimée depuis) n'est pas
/// réécrite telle quelle : elle deviendrait une référence morte.
#[test]
fn destination_vers_une_page_disparue_est_abandonnee() {
    use youpdf_core::structure::{OutlineEntry, OutlineTarget};

    let mut doc = Document::from_bytes(&text_pdf(&[vec![], vec![]])).expect("ouverture");
    let ghost = OutlineTarget::Dest(Object::Array(vec![
        Object::Reference((9999, 0)),
        Object::Name(b"Fit".to_vec()),
    ]));
    let tree = vec![OutlineEntry {
        title: "Fantôme".to_owned(),
        page: None,
        target: ghost,
        children: Vec::new(),
    }];
    doc.write_outline(&tree).expect("écriture");

    let back = doc.outline_tree();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].title, "Fantôme");
    assert_eq!(
        back[0].target,
        OutlineTarget::None,
        "la destination morte est remplacée par une destination nulle"
    );
    assert_eq!(back[0].page, None);
}
