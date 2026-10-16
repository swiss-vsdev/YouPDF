//! Dates PDF : lecture des métadonnées et écriture de `/M`.
//!
//! Une date PDF s'écrit `D:AAAAMMJJHHmmSS` suivie d'un décalage, pas comme
//! une chaîne ISO. Les deux sens doivent s'accorder : ce qu'on écrit doit se
//! relire.

use lopdf::{Dictionary, Object, Stream, dictionary};
use youpdf_core::{Annotation, AnnotationKind, Document};

/// PDF d'une page blanche, avec les métadonnées demandées.
fn page_avec_info(entrees: &[(&str, &str)]) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let content = doc.add_object(Stream::new(Dictionary::new(), b" ".to_vec()));
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
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));

    let mut info = Dictionary::new();
    for (key, value) in entrees {
        info.set(*key, Object::string_literal(*value));
    }
    let info_id = doc.add_object(info);
    doc.trailer.set("Info", Object::Reference(info_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Les dates déclarées sont lues, décalage horaire compris.
#[test]
fn metadonnees_lisent_les_dates_pdf() {
    let bytes = page_avec_info(&[
        ("CreationDate", "D:20240102030405Z"),
        ("ModDate", "D:20240102030405+02'00'"),
    ]);
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let meta = doc.metadata();

    assert_eq!(
        meta.created_at.map(|d| d.to_rfc3339()),
        Some("2024-01-02T03:04:05+00:00".to_owned()),
        "date de création mal lue"
    );
    assert_eq!(
        meta.modified_at.map(|d| d.to_rfc3339()),
        Some("2024-01-02T01:04:05+00:00".to_owned()),
        "décalage horaire ignoré"
    );
}

/// Une date tronquée reste exploitable, une date absurde est ignorée.
#[test]
fn dates_tronquees_et_absurdes() {
    let bytes = page_avec_info(&[("CreationDate", "D:2024"), ("ModDate", "n'importe quoi")]);
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let meta = doc.metadata();

    assert_eq!(
        meta.created_at.map(|d| d.to_rfc3339()),
        Some("2024-01-01T00:00:00+00:00".to_owned())
    );
    assert!(meta.modified_at.is_none(), "date absurde acceptée");
}

/// `/M` est écrit au format PDF, et se relit à l'identique.
#[test]
fn annotation_ecrit_une_date_pdf() {
    let bytes = page_avec_info(&[]);
    let mut doc = Document::from_bytes(&bytes).expect("ouverture");

    let mut annot = Annotation::new(
        1,
        AnnotationKind::Highlight,
        [72.0, 700.0, 200.0, 720.0],
    );
    annot.created_at = "2024-03-04T05:06:07+00:00".to_owned();
    doc.apply_annotations(&[annot]).expect("application");

    let saved = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&saved);
    assert!(
        text.contains("/M(D:20240304050607Z)"),
        "date non conforme dans /M"
    );
    assert!(
        !text.contains("2024-03-04T05:06:07"),
        "chaîne ISO écrite dans /M"
    );

    // Relecture : la date revient sous sa forme lisible.
    let reopened = Document::from_bytes(&saved).expect("réouverture");
    let annots = reopened.read_annotations();
    assert_eq!(
        annots.first().map(|a| a.created_at.clone()),
        Some("2024-03-04T05:06:07+00:00".to_owned()),
        "date illisible à la relecture"
    );
}

/// Un PDF sans métadonnées n'invente pas de dates.
#[test]
fn pdf_sans_metadonnees() {
    let bytes = page_avec_info(&[]);
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    assert!(doc.metadata().created_at.is_none());
    assert!(doc.metadata().modified_at.is_none());
}
