//! Champs hérités et widgets séparés.
//!
//! Beaucoup de producteurs écrivent `/FT`, `/Ff` et `/V` sur le champ parent
//! et laissent les widgets (les annotations) sans `/T` ni `/FT`. Ces tests
//! vérifient que ces formulaires restent lisibles et remplissables.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream, StringFormat};
use youpdf_core::{Document, FormFieldKind};

fn rect(x1: f32, y1: f32, x2: f32, y2: f32) -> Object {
    Object::Array(vec![
        Object::Real(x1),
        Object::Real(y1),
        Object::Real(x2),
        Object::Real(y2),
    ])
}

#[test]
fn lit_un_champ_dont_le_widget_herite_de_tout() {
    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let widget = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "Rect" => rect(72.0, 700.0, 300.0, 724.0),
        "F" => 4,
    });
    let parent = doc.add_object(dictionary! {
        "T" => Object::string_literal(b"Groupe".to_vec()),
        "FT" => Object::Name(b"Tx".to_vec()),
        "Ff" => 4096,
        "V" => Object::String(b"herite".to_vec(), StringFormat::Literal),
        "Kids" => Object::Array(vec![widget.into()]),
    });
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Annots" => Object::Array(vec![widget.into()]),
        "Resources" => dictionary! {},
    });
    let acro = doc.add_object(dictionary! {
        "Fields" => Object::Array(vec![parent.into()]),
        "NeedAppearances" => true,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "AcroForm" => Object::Reference(acro),
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");
    let mut document = Document::from_bytes(&bytes).expect("ouverture");

    let fields = document.read_form_fields();
    assert_eq!(fields.len(), 1, "{fields:?}");
    let field = &fields[0];
    assert_eq!(field.name, "Groupe");
    assert_eq!(field.kind, FormFieldKind::Text);
    assert_eq!(field.value.as_deref(), Some("herite"));
    assert!(field.multiline);
    assert_eq!(field.page, Some(1));
    assert!(field.rect[2] > field.rect[0]);

    document
        .fill_form_field(field, "nouveau")
        .expect("remplissage");
    let relu = document.read_form_fields();
    assert_eq!(relu[0].value.as_deref(), Some("nouveau"));
}

#[test]
fn case_a_cocher_garde_son_etat_reel() {
    let mut doc = LopdfDoc::with_version("1.7");
    let off = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let on = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let widget = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "Rect" => rect(72.0, 650.0, 90.0, 668.0),
        "P" => Object::Reference((3, 0)),
        "AS" => Object::Name(b"On".to_vec()),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => Object::Reference(off),
                "On" => Object::Reference(on),
            },
        },
    });
    let field = doc.add_object(dictionary! {
        "T" => Object::string_literal(b"Accepte".to_vec()),
        "FT" => Object::Name(b"Btn".to_vec()),
        "V" => Object::Name(b"On".to_vec()),
        "Kids" => Object::Array(vec![widget.into()]),
    });
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Annots" => Object::Array(vec![widget.into()]),
        "Resources" => dictionary! {},
    });
    let acro = doc.add_object(dictionary! {
        "Fields" => Object::Array(vec![field.into()]),
        "NeedAppearances" => true,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "AcroForm" => Object::Reference(acro),
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");
    let mut document = Document::from_bytes(&bytes).expect("ouverture");

    let fields = document.read_form_fields();
    assert_eq!(fields.len(), 1, "{fields:?}");
    assert_eq!(fields[0].kind, FormFieldKind::Checkbox);
    assert_eq!(fields[0].states, vec!["On".to_string()]);
    assert_eq!(fields[0].state.as_deref(), Some("On"));

    document
        .fill_form_field(&fields[0], "Off")
        .expect("décocher");
    let relu = document.read_form_fields();
    assert_eq!(relu[0].value.as_deref(), Some("Off"));
}

/// Widget radio avec ses deux apparences, à l'abscisse demandée.
fn radio_widget(doc: &mut LopdfDoc, state: &str, x: f32) -> Object {
    let off = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let on = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let mut appearances = lopdf::Dictionary::new();
    appearances.set("Off", Object::Reference(off));
    appearances.set(state, Object::Reference(on));
    let mut ap = lopdf::Dictionary::new();
    ap.set("N", Object::Dictionary(appearances));
    let widget = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "Rect" => rect(x, 600.0, x + 18.0, 618.0),
        "AS" => Object::Name(state.as_bytes().to_vec()),
        "AP" => Object::Dictionary(ap),
    });
    Object::Reference(widget)
}

#[test]
fn groupe_radio_a_un_widget_par_bouton() {
    let mut doc = LopdfDoc::with_version("1.7");
    let first = radio_widget(&mut doc, "A", 72.0);
    let second = radio_widget(&mut doc, "B", 100.0);
    let field = doc.add_object(dictionary! {
        "T" => Object::string_literal(b"Choix".to_vec()),
        "FT" => Object::Name(b"Btn".to_vec()),
        "Ff" => 32768,
        "V" => Object::Name(b"A".to_vec()),
        "Kids" => Object::Array(vec![first.clone(), second.clone()]),
    });
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Annots" => Object::Array(vec![first, second]),
        "Resources" => dictionary! {},
    });
    let acro = doc.add_object(dictionary! {
        "Fields" => Object::Array(vec![field.into()]),
        "NeedAppearances" => true,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "AcroForm" => Object::Reference(acro),
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");
    let mut document = Document::from_bytes(&bytes).expect("ouverture");

    let fields = document.read_form_fields();
    assert_eq!(fields.len(), 2, "{fields:?}");
    assert!(fields.iter().all(|field| field.kind == FormFieldKind::Radio));
    assert_eq!(fields[0].states, vec!["A".to_string()]);
    assert_eq!(fields[1].states, vec!["B".to_string()]);
    assert_eq!(fields[0].page, Some(1));

    document
        .fill_form_field(&fields[1], "B")
        .expect("choisir B");
    let relu = document.read_form_fields();
    assert_eq!(relu[0].value.as_deref(), Some("B"));
    assert_eq!(relu[0].state.as_deref(), Some("Off"));
    assert_eq!(relu[1].state.as_deref(), Some("B"));
}
