//! Test de fumée de l'organisation des pages et de la structure du document.
//!
//! Chaque test échoue si la logique de mutation casse : comptage des pages,
//! rotation, insertion d'un autre document, extraction, découpe, plan et
//! pièces jointes.

use std::collections::BTreeMap;

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::Document;

/// Construit un PDF de `pages` pages vides et renvoie ses octets.
fn multipage_pdf(pages: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for _ in 0..pages {
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        });
        kids.push(page_id.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => pages as i64,
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

/// PDF d'une page avec un plan (signets) et une pièce jointe embarquée.
fn structured_pdf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page_id.into()]),
        "Count" => 1,
    });

    // Plan : « Chapitre 1 » -> page 1.
    let outline_root = doc.new_object_id();
    let outline_item = doc.add_object(dictionary! {
        "Title" => Object::string_literal("Chapitre 1"),
        "Parent" => outline_root,
        "Dest" => Object::Array(vec![page_id.into(), Object::Name(b"Fit".to_vec())]),
    });
    doc.set_object(
        outline_root,
        dictionary! {
            "Type" => Object::Name(b"Outlines".to_vec()),
            "First" => outline_item,
            "Last" => outline_item,
            "Count" => 1,
        },
    );

    // Pièce jointe embarquée : fichier texte.
    let payload = b"contenu joint".to_vec();
    let file_stream = doc.add_object(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"EmbeddedFile".to_vec()),
            "Subtype" => Object::Name(b"text/plain".to_vec()),
        },
        payload.clone(),
    ));
    let filespec = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("note.txt"),
        "UF" => Object::string_literal("note.txt"),
        "Desc" => Object::string_literal("note de test"),
        "EF" => dictionary! { "F" => file_stream, "UF" => file_stream },
    });
    let embedded = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal("note.txt"),
            filespec.into(),
        ]),
    });

    let script = doc.add_object(dictionary! {
        "S" => Object::Name(b"JavaScript".to_vec()),
        "JS" => Object::string_literal("app.alert('coucou')"),
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
        "Outlines" => outline_root,
        "Names" => dictionary! { "EmbeddedFiles" => embedded, "JavaScript" => script },
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("youpdf-organize-tests");
    std::fs::create_dir_all(&dir).expect("dossier temporaire");
    dir.join(name)
}

#[test]
fn rotate_and_delete_pages() {
    let bytes = multipage_pdf(5).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(doc.page_count(), 5);

    let rotated = doc.rotate_pages(&[1, 2], 90).expect("rotation");
    assert_eq!(rotated, 2, "deux pages tournées");
    let rotated_again = doc.rotate_pages(&[1], 270).expect("rotation inverse");
    assert_eq!(rotated_again, 1);

    let removed = doc.delete_pages(&[2, 4]).expect("suppression");
    assert_eq!(removed, 2);
    assert_eq!(doc.page_count(), 3, "trois pages restantes");

    // Sauvegarde puis relecture : la suppression doit survivre au disque.
    let path = temp_path("deleted.pdf");
    doc.save(&path).expect("sauvegarde");
    let reopened = Document::open(&path).expect("relecture");
    assert_eq!(reopened.page_count(), 3);

    // Supprimer toutes les pages doit être refusé.
    assert!(doc.delete_pages(&[1, 2, 3]).is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn rotation_changes_displayed_page_size() {
    let bytes = multipage_pdf(2).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(doc.page_size(1), (612.0, 792.0), "portrait au départ");

    doc.rotate_pages(&[1], 90).expect("rotation");
    assert_eq!(
        doc.page_size(1),
        (792.0, 612.0),
        "une page tournée à 90° s'affiche en paysage"
    );
    assert_eq!(doc.page_size(2), (612.0, 792.0), "la page 2 est intacte");

    doc.rotate_pages(&[1], 90).expect("rotation");
    assert_eq!(doc.page_size(1), (612.0, 792.0), "180° : portrait");

    doc.rotate_pages(&[1], 270).expect("rotation");
    assert_eq!(doc.page_size(1), (792.0, 612.0), "270° : paysage");
}

#[test]
fn insert_and_extract_pages() {
    let base = multipage_pdf(2).expect("PDF de base");
    let extra = multipage_pdf(3).expect("PDF à insérer");
    let mut doc = Document::from_bytes(&base).expect("ouverture");
    let source = temp_path("insert.pdf");
    std::fs::write(&source, &extra).expect("écriture du source");

    let inserted = doc.insert_pages_after(1, &source).expect("insertion");
    assert_eq!(inserted, 3);
    assert_eq!(doc.page_count(), 5);

    // Insertion en tête.
    let inserted = doc.insert_pages_after(0, &source).expect("insertion en tête");
    assert_eq!(inserted, 3);
    assert_eq!(doc.page_count(), 8);

    // Extraction de deux pages vers un nouveau fichier.
    let target = temp_path("extract.pdf");
    doc.extract_pages(&[1, 2], &target).expect("extraction");
    let extracted = Document::open(&target).expect("relecture du fichier extrait");
    assert_eq!(extracted.page_count(), 2);

    // Découpe : 5 pages par paquets de 2 -> 3 fichiers.
    let dir = temp_path("split");
    std::fs::create_dir_all(&dir).expect("dossier de découpe");
    let written = doc.split(&dir, 2).expect("découpe");
    assert_eq!(written.len(), 4, "8 pages / 2 = 4 fichiers");
    for path in &written {
        let part = Document::open(path).expect("relecture d'un morceau");
        assert!(part.page_count() <= 2);
        let _ = std::fs::remove_file(path);
    }

    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(target);
}

#[test]
fn outline_and_attachments() {
    let bytes = structured_pdf().expect("PDF structuré");
    let doc = Document::from_bytes(&bytes).expect("ouverture");

    let outline = doc.outline();
    assert_eq!(outline.len(), 1, "un signet");
    assert_eq!(outline[0].title, "Chapitre 1");
    assert_eq!(outline[0].page, Some(1), "signet vers la page 1");
    assert_eq!(outline[0].level, 0);

    let attachments = doc.attachments();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].name, "note.txt");
    assert_eq!(attachments[0].description.as_deref(), Some("note de test"));

    let bytes = doc.attachment_bytes(0).expect("contenu joint");
    assert_eq!(bytes, b"contenu joint");
    assert!(doc.attachment_bytes(3).is_err(), "index hors limites");
}

#[test]
fn sanitize_removes_scripts_and_keeps_attachments() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(!doc.attachments().is_empty());

    let removed = doc.sanitize().expect("nettoyage");
    assert!(removed >= 1, "au moins une entrée retirée");

    // Le JavaScript est parti…
    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");
    let names = reopened
        .catalog()
        .expect("catalogue")
        .get(b"Names")
        .expect("entrée Names")
        .as_dict()
        .expect("dictionnaire Names");
    assert!(
        names.get(b"JavaScript").is_err(),
        "le script est resté sous /Names"
    );
    // …mais la pièce jointe, qui vit sous le même `/Names`, est restée.
    assert!(
        !doc.attachments().is_empty(),
        "le nettoyage a emporté les pièces jointes"
    );
    assert_eq!(doc.attachment_bytes(0).expect("contenu joint"), b"contenu joint");
}

#[test]
fn page_ids_are_in_order() {
    let bytes = multipage_pdf(3).expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let ids: BTreeMap<u32, lopdf::ObjectId> = doc.page_ids();
    assert_eq!(ids.len(), 3);
    assert!(ids[&1].0 < ids[&3].0, "identifiants croissants");
}

/// Un arbre `/Names` abîmé ne doit ni paniquer ni boucler.
///
/// Les producteurs réels écrivent parfois un tableau impair (nom sans sa
/// référence) et un `/Kids` qui se référence lui-même.
#[test]
fn pieces_jointes_arbre_abime() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let body = doc.add_object(Stream::new(dictionary! {}, b"corps".to_vec()));
    let spec = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal(b"orphelin.txt".to_vec()),
        "EF" => dictionary! { "F" => body },
    });
    let tree = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal(b"orphelin.txt".to_vec()),
            Object::Reference(spec),
            Object::string_literal(b"paire_manquante".to_vec()),
        ]),
        "Kids" => Object::Array(vec![]),
    });
    // Cycle : le nœud se déclare son propre enfant.
    doc.get_object_mut(tree)
        .expect("nœud")
        .as_dict_mut()
        .expect("dictionnaire")
        .set("Kids", Object::Array(vec![Object::Reference(tree)]));
    let names = doc.add_object(dictionary! { "EmbeddedFiles" => tree });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "Names" => names,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture du PDF de test");

    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let attachments = doc.attachments();
    assert_eq!(attachments.len(), 1, "la paire complète reste lisible");
    assert_eq!(attachments[0].name, "orphelin.txt");
    assert_eq!(
        doc.attachment_bytes(0).expect("contenu joint"),
        b"corps".to_vec()
    );
    assert!(doc.attachment_bytes(1).is_err(), "index hors limites");
}

/// PDF de `pages` pages vides, de la taille demandée.
fn pdf_of_size(pages: usize, width: i64, height: i64) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for _ in 0..pages {
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), width.into(), height.into()]),
        });
        kids.push(page_id.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => pages as i64,
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");
    buf
}

/// « Insérer après la page N », avec N au-delà de la dernière, ajoute à la
/// fin. L'ancien code traitait ce cas comme « en tête ».
#[test]
fn insert_after_the_last_page_appends() {
    let mut doc = Document::from_bytes(&pdf_of_size(2, 612, 792)).expect("ouverture");
    let source = temp_path("append.pdf");
    std::fs::write(&source, pdf_of_size(3, 300, 400)).expect("écriture du source");

    doc.insert_pages_after(99, &source).expect("insertion");
    assert_eq!(doc.page_count(), 5);
    assert_eq!(doc.page_size(1), (612.0, 792.0), "la page 1 a bougé");
    assert_eq!(
        doc.page_size(5),
        (300.0, 400.0),
        "les pages ajoutées ne sont pas à la fin"
    );
    let _ = std::fs::remove_file(source);
}

/// Les pages insérées dans un sous-arbre s'y rattachent par `/Parent`.
///
/// Les figer sur la racine alors qu'elles vivent plus bas rend l'arbre
/// incohérent pour tout lecteur qui suit `/Parent`.
#[test]
fn inserted_pages_point_to_the_node_that_holds_them() {
    // Arbre imbriqué : racine -> sous-arbre -> deux pages.
    let mut builder = LopdfDoc::with_version("1.7");
    let p1 = builder.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let p2 = builder.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let sub = builder.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![p1.into(), p2.into()]),
        "Count" => 2,
    });
    let root = builder.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![sub.into()]),
        "Count" => 2,
    });
    if let Ok(Object::Dictionary(dict)) = builder.get_object_mut(p1) {
        dict.set("Parent", Object::Reference(sub));
    }
    if let Ok(Object::Dictionary(dict)) = builder.get_object_mut(p2) {
        dict.set("Parent", Object::Reference(sub));
    }
    if let Ok(Object::Dictionary(dict)) = builder.get_object_mut(sub) {
        dict.set("Parent", Object::Reference(root));
    }
    let catalog = builder.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => root,
    });
    builder.trailer.set("Root", Object::Reference(catalog));
    let mut base = Vec::new();
    builder.save_to(&mut base).expect("écriture");

    let mut doc = Document::from_bytes(&base).expect("ouverture");
    let source = temp_path("nested.pdf");
    std::fs::write(&source, pdf_of_size(1, 200, 200)).expect("écriture du source");

    // Après la page 1, qui vit dans le sous-arbre.
    doc.insert_pages_after(1, &source).expect("insertion");
    assert_eq!(doc.page_count(), 3);

    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");
    let pages = reopened.get_pages();
    let inserted = *pages.get(&2).expect("page insérée");
    let parent = reopened
        .get_dictionary(inserted)
        .expect("page")
        .get(b"Parent")
        .expect("Parent")
        .as_reference()
        .expect("référence");
    let parent_dict = reopened.get_dictionary(parent).expect("parent");
    let kids = parent_dict
        .get(b"Kids")
        .expect("Kids")
        .as_array()
        .expect("tableau");
    assert!(
        kids.iter().any(|kid| kid.as_reference().ok() == Some(inserted)),
        "la page insérée n'est pas dans le /Kids de son /Parent"
    );
    let _ = std::fs::remove_file(source);
}

/// PDF de `count` pages, chacune d'une largeur distincte (600, 601, …) : le
/// numéro de page se lit dans sa taille, même après un déplacement.
fn ordered_pdf(count: usize) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut kids: Vec<Object> = Vec::new();
    for index in 0..count {
        let page_id = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                0.into(),
                0.into(),
                (600 + index as i64).into(),
                792.into(),
            ]),
        });
        kids.push(page_id.into());
    }
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => count as i64,
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");
    buf
}

/// Largeurs de toutes les pages, dans l'ordre du document.
fn widths(doc: &Document) -> Vec<f32> {
    (1..=doc.page_count() as u32)
        .map(|number| doc.page_size(number).0)
        .collect()
}

/// Page blanche insérée au milieu, duplication et déplacement gardent l'ordre
/// demandé et les dimensions de chaque page.
#[test]
fn insert_blank_duplicate_and_move_pages() {
    let mut doc = Document::from_bytes(&ordered_pdf(3)).expect("ouverture");
    assert_eq!(widths(&doc), vec![600.0, 601.0, 602.0]);

    doc.insert_blank_page_after(1, 400.0, 500.0)
        .expect("insertion");
    assert_eq!(widths(&doc), vec![600.0, 400.0, 601.0, 602.0]);
    assert_eq!(
        doc.page_size(2),
        (400.0, 500.0),
        "le format demandé est respecté"
    );

    doc.duplicate_pages(&[3]).expect("duplication");
    assert_eq!(widths(&doc), vec![600.0, 400.0, 601.0, 601.0, 602.0]);

    doc.move_page(1, 3).expect("déplacement vers l'avant");
    assert_eq!(widths(&doc), vec![400.0, 601.0, 600.0, 601.0, 602.0]);

    doc.move_page(5, 1).expect("déplacement vers l'arrière");
    assert_eq!(widths(&doc), vec![602.0, 400.0, 601.0, 600.0, 601.0]);

    doc.move_page(0, 2).expect("page 0 est un no-op");
    assert_eq!(widths(&doc), vec![602.0, 400.0, 601.0, 600.0, 601.0]);
}

/// PDF à deux sous-arbres : `Pages` racine → `[Pages A → [1, 2]]`,
/// `[Pages B → [3, 4]]`. La page 3 vit dans le second sous-arbre.
fn two_branch_pdf() -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page = |doc: &mut LopdfDoc, width: i64| {
        doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), width.into(), 792.into()]),
        })
    };
    let p1 = page(&mut doc, 601);
    let p2 = page(&mut doc, 602);
    let p3 = page(&mut doc, 603);
    let p4 = page(&mut doc, 604);
    let branch_a = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![p1.into(), p2.into()]),
        "Count" => 2,
    });
    let branch_b = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![p3.into(), p4.into()]),
        "Count" => 2,
    });
    for (id, parent) in [(p1, branch_a), (p2, branch_a), (p3, branch_b), (p4, branch_b)] {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(id) {
            dict.set("Parent", Object::Reference(parent));
        }
    }
    if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(branch_a) {
        dict.set("Parent", Object::Reference((0, 0)));
    }
    let root = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![branch_a.into(), branch_b.into()]),
        "Count" => 4,
    });
    for branch in [branch_a, branch_b] {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(branch) {
            dict.set("Parent", Object::Reference(root));
        }
    }
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => root,
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");
    buf
}

/// Une ancre présente dans le second sous-arbre ne doit pas être prise pour
/// une ancre absente : la page insérée doit atterrir dans ce sous-arbre.
#[test]
fn insert_after_a_page_of_the_second_subtree() {
    let mut doc = Document::from_bytes(&two_branch_pdf()).expect("ouverture");
    assert_eq!(widths(&doc), vec![601.0, 602.0, 603.0, 604.0]);

    doc.insert_blank_page_after(3, 400.0, 500.0)
        .expect("insertion");
    assert_eq!(
        widths(&doc),
        vec![601.0, 602.0, 603.0, 400.0, 604.0],
        "la page blanche doit suivre la page 3, dans son sous-arbre"
    );

    let out = doc.to_bytes().expect("sérialisation");
    let reopened = lopdf::Document::load_mem(&out).expect("réouverture");
    let pages = reopened.get_pages();
    let inserted = *pages.get(&4).expect("page insérée");
    let parent = reopened
        .get_dictionary(inserted)
        .expect("page")
        .get(b"Parent")
        .expect("Parent")
        .as_reference()
        .expect("référence");
    let kids = reopened
        .get_dictionary(parent)
        .expect("parent")
        .get(b"Kids")
        .expect("Kids")
        .as_array()
        .expect("tableau")
        .len();
    assert_eq!(kids, 3, "la page blanche rejoint le sous-arbre de la page 3");
}

/// Déplacer une page d'un sous-arbre à l'autre conserve l'objet de page : les
/// références entrantes restent valides et rien ne devient orphelin.
#[test]
fn move_page_across_subtrees_keeps_the_object() {
    let mut doc = Document::from_bytes(&two_branch_pdf()).expect("ouverture");
    let before: Vec<lopdf::ObjectId> = doc.page_ids().into_values().collect();

    doc.move_page(4, 1).expect("déplacement");
    assert_eq!(widths(&doc), vec![604.0, 601.0, 602.0, 603.0]);

    let after: Vec<lopdf::ObjectId> = doc.page_ids().into_values().collect();
    assert_eq!(
        after[0], before[3],
        "la page déplacée garde son identifiant d'objet"
    );
    let mut sorted = after.clone();
    sorted.sort();
    let mut expected = before.clone();
    expected.sort();
    assert_eq!(sorted, expected, "aucune page n'est perdue ni dupliquée");
}

/// Une pièce jointe ajoutée se relit, description comprise.
#[test]
fn ajout_de_piece_jointe_puis_relecture() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    doc.add_attachment("table.csv", Some("relevé"), b"a,b\n")
        .expect("ajout");

    let attachments = doc.attachments();
    assert_eq!(attachments.len(), 2, "l'ancienne reste, la nouvelle arrive");
    assert_eq!(attachments[1].name, "table.csv");
    assert_eq!(attachments[1].description.as_deref(), Some("relevé"));
    assert_eq!(attachments[1].size, Some(4));
    assert_eq!(doc.attachment_bytes(1).expect("contenu"), b"a,b\n");
    // L'ancienne n'a pas bougé.
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"contenu joint");

    // Le fichier écrit se relit comme un PDF normal.
    let round = Document::from_bytes(&doc.to_bytes().expect("écriture")).expect("réouverture");
    assert_eq!(round.attachments().len(), 2);
    assert_eq!(round.attachment_bytes(1).expect("contenu"), b"a,b\n");
}

/// Ajouter un nom déjà présent remplace l'entrée au lieu de la dupliquer.
#[test]
fn ajout_du_meme_nom_remplace_l_entree() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    doc.add_attachment("note.txt", None, b"nouveau contenu")
        .expect("ajout");

    let attachments = doc.attachments();
    assert_eq!(attachments.len(), 1, "une seule entrée porte ce nom");
    assert_eq!(attachments[0].name, "note.txt");
    assert_eq!(
        doc.attachment_bytes(0).expect("contenu"),
        b"nouveau contenu"
    );
}

/// Le retrait enlève l'entrée, son descripteur et son flux.
#[test]
fn retrait_de_piece_jointe_libere_les_objets() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    doc.remove_attachment(0).expect("retrait");

    assert!(doc.attachments().is_empty(), "plus aucune pièce jointe");
    assert!(doc.attachment_bytes(0).is_err(), "plus de contenu à lire");
    // Aucun descripteur ni flux embarqué ne subsiste dans le fichier.
    let written = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&written);
    assert!(!text.contains("Filespec"), "descripteur resté dans le fichier");
    assert!(!text.contains("EmbeddedFile"), "flux resté dans le fichier");
    // Le reste de la structure est intact.
    let round = Document::from_bytes(&written).expect("réouverture");
    assert_eq!(round.outline().len(), 1);
    assert_eq!(round.page_count(), 1);
}

/// Un index hors limites est refusé, sans toucher au document.
#[test]
fn retrait_hors_limites_est_refuse() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    assert!(doc.remove_attachment(4).is_err());
    assert_eq!(doc.attachments().len(), 1, "rien n'a été retiré");
}

/// L'arbre à plusieurs nœuds est aplati sans perdre d'entrée.
#[test]
fn ajout_dans_un_arbre_a_plusieurs_noeuds() {
    let bytes = nested_attachments_pdf().expect("PDF à deux nœuds");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(doc.attachments().len(), 2, "les deux nœuds sont lus");

    doc.add_attachment("c.txt", None, b"troisieme")
        .expect("ajout");
    assert_eq!(doc.attachments().len(), 3);
    assert_eq!(doc.attachment_bytes(2).expect("contenu"), b"troisieme");

    doc.remove_attachment(1).expect("retrait");
    let names: Vec<String> = doc.attachments().into_iter().map(|a| a.name).collect();
    assert_eq!(names, vec!["a.txt".to_owned(), "c.txt".to_owned()]);
    assert_eq!(doc.attachment_bytes(1).expect("contenu"), b"troisieme");
}

/// PDF dont l'arbre `/EmbeddedFiles` a deux nœuds feuilles.
fn nested_attachments_pdf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page_id.into()]),
        "Count" => 1,
    });

    let mut leaves: Vec<Object> = Vec::new();
    for (name, payload) in [("a.txt", b"premier".to_vec()), ("b.txt", b"second".to_vec())] {
        let stream = doc.add_object(Stream::new(dictionary! {}, payload));
        let filespec = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Filespec".to_vec()),
            "F" => Object::string_literal(name),
            "UF" => Object::string_literal(name),
            "EF" => dictionary! { "F" => stream, "UF" => stream },
        });
        let leaf = doc.add_object(dictionary! {
            "Names" => Object::Array(vec![Object::string_literal(name), filespec.into()]),
        });
        leaves.push(leaf.into());
    }
    let root = doc.add_object(dictionary! {
        "Kids" => Object::Array(leaves),
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
        "Names" => dictionary! { "EmbeddedFiles" => root },
    });
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

/// PDF dont l'arbre mélange un descripteur en ligne et un descripteur
/// indirect visant le même flux embarqué.
fn tricky_attachments_pdf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });

    let shared = doc.add_object(Stream::new(dictionary! {}, b"partage".to_vec()));
    let inline = dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("inline.txt"),
        "UF" => Object::string_literal("inline.txt"),
        "EF" => dictionary! { "F" => shared, "UF" => shared },
    };
    let indirect = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("indirect.txt"),
        "UF" => Object::string_literal("indirect.txt"),
        "EF" => dictionary! { "F" => shared, "UF" => shared },
    });
    let tree = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal("inline.txt"),
            Object::Dictionary(inline),
            Object::string_literal("indirect.txt"),
            Object::Reference(indirect),
        ]),
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "Names" => dictionary! { "EmbeddedFiles" => tree },
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

/// Un descripteur en ligne est listé, extrait et retiré comme les autres.
#[test]
fn descripteur_en_ligne_est_lu_extrait_et_retire() {
    let bytes = tricky_attachments_pdf().expect("PDF");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    let names: Vec<String> = doc.attachments().into_iter().map(|a| a.name).collect();
    assert_eq!(names, vec!["inline.txt".to_owned(), "indirect.txt".to_owned()]);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"partage");

    doc.remove_attachment(0).expect("retrait");
    let names: Vec<String> = doc.attachments().into_iter().map(|a| a.name).collect();
    assert_eq!(names, vec!["indirect.txt".to_owned()]);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"partage");
}

/// Deux descripteurs partageant un flux gardent leurs index, et le flux
/// partagé survit au retrait de l'un d'eux.
#[test]
fn deux_descripteurs_partageant_un_flux_gardent_leur_index() {
    let bytes = tricky_attachments_pdf().expect("PDF");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    assert_eq!(doc.attachments().len(), 2);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"partage");
    assert_eq!(doc.attachment_bytes(1).expect("contenu"), b"partage");

    doc.remove_attachment(1).expect("retrait");
    assert_eq!(doc.attachments().len(), 1);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"partage");
}

/// Remplacer une pièce jointe purge son ancien descripteur et son flux, sans
/// emporter les autres entrées de `/Names`.
#[test]
fn remplacement_purge_l_ancien_descripteur() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    doc.add_attachment("note.txt", None, b"remplace")
        .expect("remplacement");
    assert_eq!(doc.attachments().len(), 1);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"remplace");

    let written = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&written);
    assert!(!text.contains("contenu joint"), "l'ancien flux est resté");
    // Le reste de `/Names` (ici un script) n'est pas emporté.
    assert!(text.contains("JavaScript"), "l'entrée voisine a disparu");
}

/// PDF dont l'arbre liste deux descripteurs sous le même nom affiché.
fn duplicate_attachment_pdf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });

    let first = doc.add_object(Stream::new(dictionary! {}, b"premier".to_vec()));
    let second = doc.add_object(Stream::new(dictionary! {}, b"second".to_vec()));
    let spec_a = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("double.txt"),
        "UF" => Object::string_literal("double.txt"),
        "EF" => dictionary! { "F" => first, "UF" => first },
    });
    let spec_b = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "F" => Object::string_literal("double.txt"),
        "UF" => Object::string_literal("double.txt"),
        "EF" => dictionary! { "F" => second, "UF" => second },
    });
    let tree = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal("cle-a"),
            Object::Reference(spec_a),
            Object::string_literal("cle-b"),
            Object::Reference(spec_b),
        ]),
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "Names" => dictionary! { "EmbeddedFiles" => tree },
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

/// PDF dont le descripteur ne porte ni `/F` ni `/UF`.
fn unnamed_attachment_pdf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });

    let body = doc.add_object(Stream::new(dictionary! {}, b"anonyme".to_vec()));
    let spec = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Filespec".to_vec()),
        "EF" => dictionary! { "F" => body, "UF" => body },
    });
    let tree = doc.add_object(dictionary! {
        "Names" => Object::Array(vec![
            Object::string_literal("cle.txt"),
            Object::Reference(spec),
        ]),
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "Names" => dictionary! { "EmbeddedFiles" => tree },
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}

/// Un descripteur sans nom affiché prend la clé de l'arbre, sinon deux
/// descripteurs anonymes se confondraient.
#[test]
fn descripteur_sans_nom_prend_la_cle_de_l_arbre() {
    let bytes = unnamed_attachment_pdf().expect("PDF");
    let doc = Document::from_bytes(&bytes).expect("ouverture");

    let names: Vec<String> = doc.attachments().into_iter().map(|a| a.name).collect();
    assert_eq!(names, vec!["cle.txt".to_owned()]);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"anonyme");
}

/// Un nom ne désigne qu'un fichier : remplacer un nom que l'arbre porte en
/// double ne laisse qu'une entrée, celle du nouveau fichier.
#[test]
fn ajout_avec_un_nom_en_double_laisse_une_seule_entree() {
    let bytes = duplicate_attachment_pdf().expect("PDF");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(doc.attachments().len(), 2);

    doc.add_attachment("double.txt", None, b"nouveau")
        .expect("remplacement");
    let names: Vec<String> = doc.attachments().into_iter().map(|a| a.name).collect();
    assert_eq!(names, vec!["double.txt".to_owned()]);
    assert_eq!(doc.attachment_bytes(0).expect("contenu"), b"nouveau");

    let written = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&written);
    assert!(!text.contains("premier"), "l'ancien flux est resté");
    assert!(!text.contains("second"), "l'ancien flux est resté");
}

/// Remplacer une plage insère les pages du fichier source à sa place.
#[test]
fn replace_pages_swaps_the_range() {
    let base = multipage_pdf(4).expect("PDF de base");
    let extra = multipage_pdf(2).expect("PDF source");
    let mut doc = Document::from_bytes(&base).expect("ouverture");
    let source = temp_path("replace.pdf");
    std::fs::write(&source, &extra).expect("écriture du source");

    let inserted = doc.replace_pages(&[2, 3], &source).expect("remplacement");
    assert_eq!(inserted, 2, "deux pages insérées");
    assert_eq!(doc.page_count(), 4, "4 - 2 + 2");

    let _ = std::fs::remove_file(source);
}

/// La découpe par taille ne produit aucun fichier plus gros que la limite.
#[test]
fn split_by_size_respects_the_limit() {
    let bytes = multipage_pdf(6).expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let dir = temp_path("split-size");
    std::fs::create_dir_all(&dir).expect("dossier de découpe");

    let files = doc.split_by_size(&dir, 4096).expect("découpe par taille");
    assert!(!files.is_empty());
    for path in &files {
        let size = std::fs::metadata(path).expect("taille").len();
        assert!(size <= 4096 || files.len() == 6, "{path:?} : {size} octets");
        let part = Document::open(path).expect("relecture");
        assert!(part.page_count() >= 1);
        let _ = std::fs::remove_file(path);
    }
}

/// Découper une page en 2 × 2 crée quatre pages à la taille de la tuile.
#[test]
fn split_tiles_makes_four_pages() {
    let bytes = multipage_pdf(3).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    let created = doc.split_tiles(&[2], 2, 2).expect("découpage");
    assert_eq!(created, 4);
    assert_eq!(doc.page_count(), 6, "3 - 1 + 4");
    // La première tuile remplace la page visée : 612 × 792 devient 306 × 396.
    let (width, height) = doc.page_size(2);
    assert!((width - 306.0).abs() < 0.01, "{width}");
    assert!((height - 396.0).abs() < 0.01, "{height}");
}

/// L'écart entre tuiles rétrécit les tuiles et laisse le blanc demandé.
#[test]
fn split_tiles_with_gap_leaves_room_between_tiles() {
    let bytes = multipage_pdf(3).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    // 2 × 2 avec 12 pt d'écart : (612 - 12) / 2 = 300 pt de large.
    let created = doc
        .split_tiles_with_gap(&[2], 2, 2, 12.0)
        .expect("découpage");
    assert_eq!(created, 4);
    let (width, height) = doc.page_size(2);
    assert!((width - 300.0).abs() < 0.01, "largeur : {width}");
    assert!((height - 390.0).abs() < 0.01, "hauteur : {height}");
}

/// Un écart nul donne le découpage exact d'origine.
#[test]
fn split_tiles_with_gap_zero_matches_plain_split() {
    let bytes = multipage_pdf(2).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    doc.split_tiles_with_gap(&[1], 2, 2, 0.0).expect("découpage");
    let (width, height) = doc.page_size(1);
    assert!((width - 306.0).abs() < 0.01, "largeur : {width}");
    assert!((height - 396.0).abs() < 0.01, "hauteur : {height}");
}

/// Un écart plus grand que la page ne produit pas de tuile de taille nulle.
#[test]
fn split_tiles_with_huge_gap_keeps_a_positive_tile() {
    let bytes = multipage_pdf(2).expect("PDF de test");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    doc.split_tiles_with_gap(&[1], 2, 2, 10_000.0)
        .expect("découpage");
    let (width, height) = doc.page_size(1);
    assert!(width >= 1.0, "largeur : {width}");
    assert!(height >= 1.0, "hauteur : {height}");
}
