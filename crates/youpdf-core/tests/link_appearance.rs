//! Tests pour l'apparence des liens : `/Border` et `/C`.

use youpdf_core::{Document, LinkAppearance};
use lopdf::{Dictionary, Object};

/// Crée un document d'une page avec une annotation `Link` au rectangle
/// `[x1, y1, x2, y2]`, puis renvoie `(doc, annot_id)`.
fn doc_with_link(x1: f32, y1: f32, x2: f32, y2: f32) -> (Document, lopdf::ObjectId) {
    let mut doc = youpdf_core::create::blank_document(1, 600.0, 800.0).expect("document vide");
    let page_id = doc.page_id(1).expect("page 1");

    let mut annot_dict = Dictionary::new();
    annot_dict.set(b"Subtype", Object::Name(b"Link".to_vec()));
    annot_dict.set(
        b"Rect",
        Object::Array(vec![
            Object::Real(x1),
            Object::Real(y1),
            Object::Real(x2),
            Object::Real(y2),
        ]),
    );

    let annot_id = doc.inner_mut().add_object(Object::Dictionary(annot_dict));

    // Ajouter la référence à l'annotation dans le tableau `/Annots` de la page.
    let mut page_dict = doc.inner_mut().get_dictionary_mut(page_id).expect("page dict");
    match page_dict.get_mut(b"Annots") {
        Ok(Object::Array(arr)) => arr.push(Object::Reference(annot_id)),
        _ => {
            page_dict.set(
                b"Annots",
                Object::Array(vec![Object::Reference(annot_id)]),
            );
        }
    }

    (doc, annot_id)
}

#[test]
fn solid_blue_border() {
    let (mut doc, _id) = doc_with_link(100.0, 100.0, 200.0, 200.0);

    let appearance = LinkAppearance::default_visible();
    youpdf_core::links::set_link_appearance(
        &mut doc,
        1,
        [100.0, 100.0, 200.0, 200.0],
        &appearance,
    )
    .expect("set appearance");

    let read_back = youpdf_core::links::get_link_appearance(&doc, 1, [100.0, 100.0, 200.0, 200.0])
        .expect("get appearance");

    assert!(read_back.visible);
    assert_eq!(read_back.width, 1.0);
    assert_eq!(read_back.color, [0, 0, 255]);
    assert_eq!(read_back.dash, None);
}

#[test]
fn dashed_invisible() {
    let (mut doc, _id) = doc_with_link(100.0, 100.0, 200.0, 200.0);

    let appearance = LinkAppearance {
        visible: false,
        width: 0.0,
        dash: Some(vec![3.0, 2.0]),
        color: [255, 0, 0],
    };

    youpdf_core::links::set_link_appearance(
        &mut doc,
        1,
        [100.0, 100.0, 200.0, 200.0],
        &appearance,
    )
    .expect("set appearance");

    let read_back = youpdf_core::links::get_link_appearance(&doc, 1, [100.0, 100.0, 200.0, 200.0])
        .expect("get appearance");

    assert!(!read_back.visible);
    assert_eq!(read_back.width, 0.0);
    assert_eq!(read_back.color, [255, 0, 0]);
    assert_eq!(read_back.dash, Some(vec![3.0, 2.0]));
}

#[test]
fn roundtrip_appearance() {
    let (mut doc, _id) = doc_with_link(50.0, 50.0, 150.0, 150.0);

    let original = LinkAppearance {
        visible: true,
        width: 2.0,
        dash: Some(vec![4.0, 2.0]),
        color: [0, 255, 0],
    };

    youpdf_core::links::set_link_appearance(
        &mut doc,
        1,
        [50.0, 50.0, 150.0, 150.0],
        &original,
    )
    .expect("set appearance");

    let read_back = youpdf_core::links::get_link_appearance(&doc, 1, [50.0, 50.0, 150.0, 150.0])
        .expect("get appearance");

    assert_eq!(read_back.visible, original.visible);
    assert_eq!(read_back.width, original.width);
    assert_eq!(read_back.dash, original.dash);
    assert_eq!(read_back.color, original.color);
}

#[test]
fn unknown_link_returns_none() {
    let (doc, _id) = doc_with_link(50.0, 50.0, 150.0, 150.0);
    let read = youpdf_core::links::get_link_appearance(&doc, 1, [10.0, 10.0, 20.0, 20.0]);
    assert!(read.is_none());
}