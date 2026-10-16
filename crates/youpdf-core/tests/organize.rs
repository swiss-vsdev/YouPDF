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

    let catalog_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages_id,
        "Outlines" => outline_root,
        "Names" => dictionary! { "EmbeddedFiles" => embedded },
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
fn sanitize_removes_metadata_and_names() {
    let bytes = structured_pdf().expect("PDF structuré");
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(!doc.attachments().is_empty());

    let removed = doc.sanitize().expect("nettoyage");
    assert!(removed >= 1, "au moins une entrée retirée");
    assert!(doc.attachments().is_empty(), "pièces jointes retirées");
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
