//! Liens d'annotation : ce qu'un clic sur un lien doit déclencher.

use lopdf::{dictionary, Dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::{Document, LinkAction};

/// PDF de deux pages, sans annotation.
fn two_pages() -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let page_ids: Vec<Object> = (0..2)
        .map(|_| {
            let stream = doc.add_object(Stream::new(dictionary! {}, b"BT ET\n".to_vec()));
            let page = doc.add_object(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
                "Resources" => dictionary! {},
                "Contents" => stream,
            });
            page.into()
        })
        .collect();
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(page_ids.clone()),
        "Count" => 2,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture du PDF de test");
    buf
}

/// Ajoute une annotation de lien à la page donnée et rend les octets.
fn with_link(annot: Dictionary) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let stream = doc.add_object(Stream::new(dictionary! {}, b"BT ET\n".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => dictionary! {},
        "Contents" => stream,
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
    doc.trailer.set("Root", catalog);
    // L'annotation est posée sur la page après sa création.
    let annot_id = doc.add_object(annot);
    if let Ok(page_dict) = doc.get_dictionary_mut(page) {
        page_dict.set("Annots", Object::Array(vec![annot_id.into()]));
    }
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture du PDF de test");
    buf
}

/// Lien vers une adresse.
fn uri_link(uri: &str) -> Dictionary {
    dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Link".to_vec()),
        "Rect" => Object::Array(vec![100.into(), 700.into(), 300.into(), 720.into()]),
        "A" => dictionary! {
            "S" => Object::Name(b"URI".to_vec()),
            "URI" => Object::string_literal(uri),
        },
    }
}

/// Un lien vers une adresse est lu comme tel.
#[test]
fn lien_uri_est_lu() {
    let bytes = with_link(uri_link("https://example.org/doc"));
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let links = doc.page_links(1);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].action, LinkAction::Uri("https://example.org/doc".to_owned()));
    assert_eq!(links[0].page, 1);
    // Le rectangle est normalisé : les coins peuvent arriver dans l'ordre.
    assert_eq!(links[0].rect, [100.0, 700.0, 300.0, 720.0]);
}

/// Une adresse vide ne donne pas de lien : elle ne déclencherait rien.
#[test]
fn lien_uri_vide_est_ignore() {
    let bytes = with_link(uri_link("   "));
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(doc.page_links(1).is_empty());
}

/// Un lien interne (`/Dest`) vise une page du document.
#[test]
fn lien_interne_vise_une_page() {
    let mut doc = LopdfDoc::with_version("1.7");
    let stream = doc.add_object(Stream::new(dictionary! {}, b"BT ET\n".to_vec()));
    let first = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => dictionary! {},
        "Contents" => stream,
    });
    let stream = doc.add_object(Stream::new(dictionary! {}, b"BT ET\n".to_vec()));
    let second = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => dictionary! {},
        "Contents" => stream,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![first.into(), second.into()]),
        "Count" => 2,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
    });
    doc.trailer.set("Root", catalog);
    let annot_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Link".to_vec()),
        "Rect" => Object::Array(vec![10.into(), 10.into(), 90.into(), 30.into()]),
        "Dest" => Object::Array(vec![second.into(), Object::Name(b"F".to_vec())]),
    });
    if let Ok(page_dict) = doc.get_dictionary_mut(first) {
        page_dict.set("Annots", Object::Array(vec![annot_id.into()]));
    }
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");

    let doc = Document::from_bytes(&buf).expect("ouverture");
    let links = doc.page_links(1);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].action, LinkAction::GoTo { page: 2 });
}

/// Une action inconnue est signalée comme telle, pas ignorée en silence.
#[test]
fn action_inconnue_est_signalee() {
    let bytes = with_link(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Link".to_vec()),
        "Rect" => Object::Array(vec![10.into(), 10.into(), 90.into(), 30.into()]),
        "A" => dictionary! {
            "S" => Object::Name(b"Launch".to_vec()),
        },
    });
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let links = doc.page_links(1);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].action, LinkAction::Unsupported("Launch".to_owned()));
}

/// Une annotation qui n'est pas un lien n'est pas listée.
#[test]
fn annotation_non_lien_est_ignoree() {
    let bytes = with_link(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Text".to_vec()),
        "Rect" => Object::Array(vec![10.into(), 10.into(), 90.into(), 30.into()]),
        "Contents" => Object::string_literal("a note"),
    });
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(doc.page_links(1).is_empty());
}

/// Une page sans lien ne donne rien.
#[test]
fn page_sans_lien_est_vide() {
    let doc = Document::from_bytes(&two_pages()).expect("ouverture");
    assert!(doc.page_links(1).is_empty());
    assert!(doc.page_links(2).is_empty());
}
