//! Seconde salve de tests de non-régression (suggestions de l'audit).
//!
//! Chaque test échouait sur le code d'origine.
use lopdf::{Dictionary, Object, Stream, dictionary};
use youpdf_core::{Document, RedactionRegion, fold_case, validate_language};

/// PDF à une page, avec des annotations : une dans la zone à caviarder,
/// une loin de celle-ci, et un volet de commentaires accroché à la première.
fn doc_with_annots() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let cid = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 100 700 Td (CONFIDENTIEL) Tj ET".to_vec(),
    ));

    // Annotation DANS la zone de caviardage [0,700 .. 300,760].
    let doomed = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Text".to_vec()),
        "Rect" => Object::Array(vec![100.into(), 710.into(), 140.into(), 750.into()]),
        "Contents" => Object::String(b"NOTE-DANS-ZONE".to_vec(), lopdf::StringFormat::Literal),
    });
    // Son volet de commentaires, sans `/Rect` : doit suivre sa maîtresse.
    let popup = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Popup".to_vec()),
        "Parent" => Object::Reference(doomed),
    });
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(doomed) {
        d.set("Popup", Object::Reference(popup));
    }

    // Annotation HORS zone.
    let keeper = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Text".to_vec()),
        "Rect" => Object::Array(vec![400.into(), 100.into(), 440.into(), 140.into()]),
        "Contents" => Object::String(b"NOTE-CONSERVEE".to_vec(), lopdf::StringFormat::Literal),
    });

    let pid = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => cid,
        "Annots" => Object::Array(vec![Object::Reference(doomed), Object::Reference(keeper)]),
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![pid.into()]),
        "Count" => 1,
    });
    let cat = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

fn annots_of(doc: &lopdf::Document) -> Vec<(String, String)> {
    let page_id = *doc.get_pages().values().next().unwrap();
    let page = doc.get_dictionary(page_id).unwrap();
    let Ok(Object::Array(items)) = page.get(b"Annots") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|o| o.as_reference().ok())
        .filter_map(|id| {
            let d = doc.get_object(id).ok()?.as_dict().ok()?;
            let kind = String::from_utf8_lossy(d.get(b"Subtype").ok()?.as_name().ok()?).into_owned();
            let text = d
                .get(b"Contents")
                .ok()
                .and_then(|o| o.as_str().ok())
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            Some((kind, text))
        })
        .collect()
}

/// Le caviardage ne doit **pas** supprimer les annotations qui ne
/// touchent pas une zone caviardée. L'original effaçait `/Annots` en
/// bloc : commentaires, tampons, signatures plates et champs de
/// formulaire disparaissaient sans avertissement.
#[test]
fn redaction_preserves_annotations_outside_the_redacted_area() {
    let Some(renderer) = youpdf_core::PdfiumRenderer::load().ok() else {
        eprintln!("pdfium indisponible : test ignoré");
        return;
    };
    let bytes = doc_with_annots();
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.redact(
        &[RedactionRegion {
            page: 1,
            rect: [0.0, 700.0, 300.0, 760.0],
        }],
        &renderer,
    )
    .expect("caviardage");

    let out = doc.to_bytes().unwrap();
    let reopened = lopdf::Document::load_mem(&out).unwrap();
    let annots = annots_of(&reopened);

    assert!(
        annots.iter().any(|(_, c)| c == "NOTE-CONSERVEE"),
        "l'annotation hors zone a été perdue : {annots:?}"
    );
    assert!(
        !annots.iter().any(|(_, c)| c == "NOTE-DANS-ZONE"),
        "une annotation couvrant la zone caviardée a survécu : {annots:?}"
    );

    // Le `/Popup` de l'annotation détruite ne traîne pas non plus.
    assert!(
        !annots.iter().any(|(k, _)| k == "Popup"),
        "volet orphelin conservé : {annots:?}"
    );
}

/// Le pliage de casse doit préserver la **longueur en octets**, sinon
/// les positions renvoyées par la recherche ne correspondent plus au
/// texte d'origine.
#[test]
fn fold_case_preserves_byte_length() {
    // Cas tordus : İ (2 octets → minuscule en 3), ẛ, ﬁ, emoji, CJK.
    for sample in [
        "ÉGLISE ÇA Ñ Ü Ω église",
        "İstanbul ẛ ﬁ 日本語 🎯",
        "’quoté“ — – ﬁ ﬂ",
        "",
        "a",
    ] {
        assert_eq!(
            fold_case(sample).len(),
            sample.len(),
            "longueur modifiée pour {sample:?}"
        );
    }
}

/// Le pliage doit être **Unicode** : `to_ascii_lowercase` ignorait les
/// accents, si bien que `église` et `ÉGLISE` ne se rencontraient jamais.
/// Attention à ne pas glisser vers la suppression des accents : ce test
/// fige le comportement choisi (casse ignorée, accents respectés).
#[test]
fn search_folds_case_beyond_ascii() {
    let mut doc = lopdf::Document::with_version("1.7");
    // Helvetica avec **WinAnsiEncoding** : c'est le cas réel que le
    // correctif vise (0xC9 = É). Une police sans encodage explicite
    // décode 0xC9 par StandardEncoding, où il n'est pas É.
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    });
    let cid = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 100 700 Td (L EGLISE ET L \xc9GLISE) Tj ET".to_vec(),
    ));
    let pid = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![pid.into()]),
        "Count" => 1,
    });
    let cat = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(cat));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let doc = Document::from_bytes(&buf).unwrap();
    let index = youpdf_core::SearchIndex::build(&doc).unwrap();

    // « ÉGLISE » : \311 = 0xC9 = É en Latin-1/WinAnsi.
    let plain = index.search("eglise", 10);
    let accented = index.search("ÉGLISE", 10);

    assert_eq!(plain.len(), 1, "`eglise` doit trouver la forme non accentuée");
    assert_eq!(
        accented.len(),
        1,
        "`ÉGLISE` doit trouver la forme accentuée (pliage Unicode)"
    );
    assert!(
        plain[0].start < accented[0].start,
        "indices décalés : la première occurrence ({}) devrait précéder la seconde ({})",
        plain[0].start,
        accented[0].start
    );
    assert!(plain[0].context.contains("EGLISE"), "{:?}", plain[0].context);
    assert!(
        accented[0].context.contains("ÉGLISE"),
        "{:?}",
        accented[0].context
    );
}

/// Les filtres de recherche : casse exacte et mots entiers.
#[test]
fn search_options_filter_matches() {
    let mut doc = lopdf::Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    // « Page page PAGES paged » : trois occurrences du mot, une collée.
    let cid = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 18 Tf 60 700 Td (Page page PAGES paged) Tj ET".to_vec(),
    ));
    let pid = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![pid.into()]),
        "Count" => 1,
    });
    let cat = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    let doc = Document::from_bytes(&buf).unwrap();
    let index = youpdf_core::SearchIndex::build(&doc).unwrap();

    let all = index.search("page", 20);
    assert_eq!(all.len(), 4, "sans filtre : les quatre formes");

    let exact = index.search_with(
        "Page",
        20,
        youpdf_core::SearchOptions {
            case_sensitive: true,
            whole_word: false,
        },
    );
    assert_eq!(exact.len(), 1, "casse exacte : une seule occurrence");

    let words = index.search_with(
        "page",
        20,
        youpdf_core::SearchOptions {
            case_sensitive: false,
            whole_word: true,
        },
    );
    assert_eq!(
        words.len(),
        2,
        "mots entiers : « PAGES » et « paged » exclus, il ne reste que les deux mots"
    );

    let both = index.search_with(
        "Page",
        20,
        youpdf_core::SearchOptions {
            case_sensitive: true,
            whole_word: true,
        },
    );
    assert_eq!(both.len(), 1, "casse exacte et mot entier : le premier « Page »");
    assert_eq!(both[0].start, 0, "et c'est bien la première occurrence");
}

/// Un OCR est souvent lancé sur un PDF venu d'ailleurs : la valeur de
/// langue, qui atterrit dans les arguments de tesseract, doit rester un
/// code de langue.
#[test]
fn ocr_language_must_be_a_language_code() {
    for ok in ["fra", "eng", "eng+fra", "script/Latn", "osd", "old_per"] {
        assert!(validate_language(ok).is_ok(), "{ok} devrait être accepté");
    }
    for bad in [
        "",
        "fra; rm -rf /",
        "--configfile=/tmp/x",
        "fra/../etc/passwd",
        "-l",
        "fra+",
        &"a".repeat(80),
    ] {
        assert!(validate_language(bad).is_err(), "{bad:?} devrait être refusé");
    }
}
