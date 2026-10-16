//! Mesures : distance, périmètre, aire.
//!
//! Les mesures sont écrites dans le flux de contenu des pages, marquées
//! `/YouPdfMeasure` : ces tests vérifient le libellé produit (échelle,
//! unité, arrondi), le retrait sélectif, et la validation des entrées.

use youpdf_core::{Document, MeasureKind, Measurement, Watermark};

/// Une page blanche 612×792, avec un flux de contenu vide.
fn page_blanche() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! {},
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Contenu concaténé d'une page (1-indexée).
fn page_content(bytes: &[u8], page_num: u32) -> Vec<u8> {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    let mut out = Vec::new();
    let Some(id) = doc.get_pages().get(&page_num).copied() else {
        return out;
    };
    let page = doc.get_dictionary(id).expect("page");
    let Ok(contents) = page.get(b"Contents") else {
        return out;
    };
    match contents {
        lopdf::Object::Reference(id) => {
            if let Ok(stream) = doc.get_object(*id).and_then(|o| o.as_stream()) {
                out.extend_from_slice(&stream.content);
            }
        }
        lopdf::Object::Array(arr) => {
            for item in arr {
                if let Ok(id) = item.as_reference() {
                    if let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) {
                        out.extend_from_slice(&stream.content);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn text_of(bytes: &[u8], page_num: u32) -> String {
    String::from_utf8_lossy(&page_content(bytes, page_num)).to_string()
}

fn mesure(kind: MeasureKind, points: &[[f32; 2]], scale: f32, unit: &str) -> Measurement {
    Measurement {
        kind,
        points: points.to_vec(),
        scale,
        unit: unit.to_string(),
    }
}

/// 72 pt à l'échelle 72 donnent 1.0 in, dans le flux de contenu.
#[test]
fn distance_libelle_en_pouces() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let m = mesure(
        MeasureKind::Distance,
        &[[100.0, 100.0], [172.0, 100.0]],
        72.0,
        "in",
    );

    doc.add_measurement(1, &m).expect("mesure");

    let bytes = doc.to_bytes().expect("écriture");
    let content = text_of(&bytes, 1);
    assert!(content.contains("1.0 in"), "libellé absent : {content:?}");
}

/// Triangle 3-4-5 à l'échelle 1 : aire 6.0 pt.
#[test]
fn aire_triangle_3_4_5() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let m = mesure(
        MeasureKind::Area,
        &[[0.0, 0.0], [3.0, 0.0], [0.0, 4.0]],
        1.0,
        "pt",
    );

    doc.add_measurement(1, &m).expect("mesure");

    let bytes = doc.to_bytes().expect("écriture");
    let content = text_of(&bytes, 1);
    assert!(content.contains("6.0 pt"), "libellé absent : {content:?}");
    assert!(content.contains("h\nS"), "polygone non fermé : {content:?}");
}

/// Le périmètre somme les segments, converti par l'échelle.
#[test]
fn perimetre_somme_segments() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let m = mesure(
        MeasureKind::Perimeter,
        &[[0.0, 0.0], [30.0, 40.0], [30.0, 0.0]],
        10.0,
        "mm",
    );

    doc.add_measurement(1, &m).expect("mesure");

    let bytes = doc.to_bytes().expect("écriture");
    let content = text_of(&bytes, 1);
    // 50 + 40 = 90 unités de dessin, / 10 = 9.0 mm.
    assert!(content.contains("9.0 mm"), "libellé absent : {content:?}");
}

/// Le retrait ne supprime que les mesures : le filigrane reste.
#[test]
fn retrait_laisse_le_filigrane() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.add_text_watermark(&Watermark::default())
        .expect("filigrane");
    doc.add_measurement(
        1,
        &mesure(MeasureKind::Distance, &[[0.0, 0.0], [10.0, 0.0]], 1.0, "pt"),
    )
    .expect("mesure");

    let retires = doc.clear_measurements().expect("retrait");
    assert_eq!(retires, 1, "un seul flux de mesure");

    let bytes = doc.to_bytes().expect("écriture");
    let content = text_of(&bytes, 1);
    assert!(!content.contains("10.0 pt"), "mesure encore présente");
    assert!(content.contains("CONFIDENTIEL"), "filigrane supprimé");
}

/// Entrées invalides : points manquants ou page hors bornes.
#[test]
fn entrees_invalides() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");

    let un_point = mesure(MeasureKind::Distance, &[[0.0, 0.0]], 1.0, "pt");
    assert!(doc.add_measurement(1, &un_point).is_err());

    let deux_points = mesure(
        MeasureKind::Area,
        &[[0.0, 0.0], [1.0, 1.0]],
        1.0,
        "pt",
    );
    assert!(doc.add_measurement(1, &deux_points).is_err());

    let valide = mesure(
        MeasureKind::Distance,
        &[[0.0, 0.0], [1.0, 0.0]],
        1.0,
        "pt",
    );
    assert!(doc.add_measurement(0, &valide).is_err());
    assert!(doc.add_measurement(2, &valide).is_err());
}
