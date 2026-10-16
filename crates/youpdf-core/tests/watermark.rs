//! Filigranes, en-têtes, pieds de page et numéros de page.
//!
//! Les décorations sont écrites dans le flux de contenu des pages, pas
//! posées en annotations : ces tests vérifient qu'elles survivent à
//! l'enregistrement, qu'elles ne touchent que les pages choisies, et que
//! les formats de numérotation sont ceux annoncés.

use youpdf_core::{Document, HeaderFooter, NumberStyle, PageRange, Watermark};

/// `n` pages blanches 612×792, chacune avec un flux de contenu vide.
fn pages_blanches(n: usize) -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });

    let mut kids = Vec::new();
    for _ in 0..n {
        let content = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page.into());
    }

    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(kids),
        "Count" => n as i64,
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

/// Contenu d'une page vu comme texte (les flux sont ASCII en pratique).
fn text_of(bytes: &[u8], page_num: u32) -> String {
    String::from_utf8_lossy(&page_content(bytes, page_num)).to_string()
}

/// Un filigrane posé sur une page blanche se relit dans les octets écrits
/// et dans le contenu de la page.
#[test]
fn filigrane_present_apres_enregistrement() {
    let mut doc = Document::from_bytes(&pages_blanches(1)).expect("ouverture");
    let wm = Watermark {
        text: "CONFIDENTIEL".to_string(),
        ..Default::default()
    };

    let posees = doc.add_text_watermark(&wm).expect("filigrane");
    assert_eq!(posees, 1);

    let bytes = doc.to_bytes().expect("écriture");
    assert!(
        String::from_utf8_lossy(&bytes).contains("CONFIDENTIEL"),
        "le texte doit être présent dans les octets enregistrés"
    );

    let content = text_of(&bytes, 1);
    assert!(content.contains("CONFIDENTIEL"), "texte absent : {content:?}");
    assert!(content.contains("YouPdfWMGS0"), "opacité absente : {content:?}");
    assert!(content.contains("Tm"), "placement du texte absent");
}

/// L'en-tête et le pied de page ne touchent que les pages de la plage.
#[test]
fn entete_pied_seulement_pages_choisies() {
    let mut doc = Document::from_bytes(&pages_blanches(3)).expect("ouverture");
    let hf = HeaderFooter {
        left: "YouPDF".to_string(),
        pages: PageRange::Range(2, 3),
        ..Default::default()
    };

    let decor = doc.add_header_footer(&hf).expect("en-tête");
    assert_eq!(decor, 2, "deux pages seulement");

    let bytes = doc.to_bytes().expect("écriture");
    assert!(
        page_content(&bytes, 1).is_empty(),
        "la page 1 ne devait pas être décorée"
    );
    assert!(text_of(&bytes, 2).contains("YouPDF"));
    assert!(text_of(&bytes, 3).contains("YouPDF"));
}

/// Les trois styles de numérotation rendent les formats annoncés.
#[test]
fn numeros_de_page_formats() {
    assert_eq!(NumberStyle::Numeric.render(1, 12), "1");
    assert_eq!(NumberStyle::NumericTotal.render(1, 12), "1 / 12");
    assert_eq!(NumberStyle::Roman.render(1, 12), "i");

    let mut doc = Document::from_bytes(&pages_blanches(2)).expect("ouverture");
    doc.add_header_footer(&HeaderFooter {
        page_numbers: Some(NumberStyle::Numeric),
        ..Default::default()
    })
    .expect("numérotation");
    let bytes = doc.to_bytes().expect("écriture");
    assert!(text_of(&bytes, 1).contains("(1) Tj"));
    assert!(text_of(&bytes, 2).contains("(2) Tj"));

    let mut doc = Document::from_bytes(&pages_blanches(2)).expect("ouverture");
    doc.add_header_footer(&HeaderFooter {
        page_numbers: Some(NumberStyle::NumericTotal),
        ..Default::default()
    })
    .expect("numérotation");
    let bytes = doc.to_bytes().expect("écriture");
    assert!(text_of(&bytes, 1).contains("(1 / 2) Tj"));

    let mut doc = Document::from_bytes(&pages_blanches(3)).expect("ouverture");
    doc.add_header_footer(&HeaderFooter {
        page_numbers: Some(NumberStyle::Roman),
        ..Default::default()
    })
    .expect("numérotation");
    let bytes = doc.to_bytes().expect("écriture");
    assert!(text_of(&bytes, 1).contains("(i) Tj"));
    assert!(text_of(&bytes, 2).contains("(ii) Tj"));
    assert!(text_of(&bytes, 3).contains("(iii) Tj"));
}

/// `skip_first_page` laisse la première page intacte.
#[test]
fn numero_ignore_premiere_page() {
    let mut doc = Document::from_bytes(&pages_blanches(3)).expect("ouverture");
    let hf = HeaderFooter {
        page_numbers: Some(NumberStyle::Numeric),
        skip_first_page: true,
        ..Default::default()
    };

    let decor = doc.add_header_footer(&hf).expect("numérotation");
    assert_eq!(decor, 2, "seules les pages 2 et 3 sont numérotées");

    let bytes = doc.to_bytes().expect("écriture");
    assert!(
        page_content(&bytes, 1).is_empty(),
        "la page 1 ne devait rien porter"
    );
    assert!(text_of(&bytes, 2).contains("(2) Tj"));
    assert!(text_of(&bytes, 3).contains("(3) Tj"));
}

/// Le retrait ne supprime que les flux marqués : le contenu d'origine reste.
#[test]
fn retrait_des_decorations() {
    let mut doc = Document::from_bytes(&pages_blanches(1)).expect("ouverture");
    doc.add_text_watermark(&Watermark::default())
        .expect("filigrane");

    let retires = doc.clear_watermarks().expect("retrait");
    assert_eq!(retires, 1);

    let bytes = doc.to_bytes().expect("écriture");
    assert!(
        !text_of(&bytes, 1).contains("CONFIDENTIEL"),
        "le filigrane devait disparaître"
    );
}

/// Une trame sur une page démesurée reste bornée, et couvre encore la page.
///
/// Sans borne, une boîte de 20 000 points de côté demandait plus de cent
/// mille motifs : des centaines de mégaoctets de flux de contenu pour une
/// seule page.
#[test]
fn trame_bornee_sur_page_demesuree() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    // Page carrée de 20 000 points de côté (≈ 7 mètres).
    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 20000.into(), 20000.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture");

    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    let wm = Watermark {
        text: "CONFIDENTIEL".to_owned(),
        tiled: true,
        ..Default::default()
    };
    doc.add_text_watermark(&wm).expect("filigrane");

    let out = doc.to_bytes().expect("sérialisation");
    let content = text_of(&out, 1);
    let stamps = content.matches("Tj").count();
    assert!(stamps >= 4, "la trame a disparu : {stamps} motif(s)");
    assert!(
        stamps <= 500,
        "la trame n'est pas bornée : {stamps} motifs"
    );
}
