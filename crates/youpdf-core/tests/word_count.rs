//! Tests pour le comptage de mots et les stats de document, plus le moteur TTS.

use youpdf_core::capture::TtsEngine;
use youpdf_core::{Document, DocumentStats, SearchIndex};

/// Construit un PDF d'une page contenant `text` (via lopdf, sans fichier).
fn tiny_pdf(text: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use lopdf::{content::Content, dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");

    let font_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });

    let content = Content {
        operations: vec![
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec!["F1".into(), 24.into()]),
            lopdf::content::Operation::new("Td", vec![100.into(), 700.into()]),
            lopdf::content::Operation::new("Tj", vec![Object::string_literal(text)]),
            lopdf::content::Operation::new("ET", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().map_err(|e| e.to_string())?,
    ));

    let page_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });

    let pages_id = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page_id.into()]),
        "Count" => 1,
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

#[test]
fn english_word_split() {
    let bytes = tiny_pdf("Hello YouPDF world").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let index = SearchIndex::build(&doc).expect("index");

    // La définition d'un « mot » est celle de `str::split_whitespace`.
    let expected = index.text().split_whitespace().count();
    assert_eq!(index.word_count(), expected, "word_count doit coïncider avec split_whitespace");
    assert_eq!(index.word_count(), 3, "trois mots : Hello, YouPDF, world");
}

#[test]
fn cjk_character_count() {
    let bytes = tiny_pdf("你好世界！").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let index = SearchIndex::build(&doc).expect("index");

    // 4 idéogrammes CJK + 1 point d'exclamation (non-CJK).
    assert_eq!(index.cjk_count(), 4, "4 caractères CJK : 你好世界");
}

#[test]
fn stats_struct() {
    let bytes = tiny_pdf("你好 world  ").expect("PDF de test");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let index = SearchIndex::build(&doc).expect("index");
    let stats = DocumentStats::from_index(&index);

    assert_eq!(stats.words, 3, "你好, world et le blanc final ne compte pas comme mot");
    assert_eq!(stats.cjk, 2, "只有 你好");
    assert_eq!(stats.total_chars, index.char_count());
    assert_eq!(stats.spaces, index.space_count());

    // Cohérence interne : les composantes sommées doivent être ≤ total_chars.
    assert!(stats.words + stats.cjk + stats.spaces <= stats.total_chars);
}

#[test]
fn tts_engine_constructs() {
    let engine = TtsEngine::new();
    assert_eq!(engine.current_voice, "Alice");
    assert!((engine.rate - 175.0).abs() < f32::EPSILON);

    // list_voices() ne doit jamais planter, même sur une plateforme sans `say`.
    let voices = TtsEngine::list_voices();
    assert!(voices.iter().all(|v| !v.is_empty()), "les voix ne doivent pas être vides");

    // set_rate borne entre 100 et 300.
    let mut engine = engine;
    engine.set_rate(500.0);
    assert!((engine.rate - 300.0).abs() < f32::EPSILON, "rate borné à 300");
    engine.set_rate(10.0);
    assert!((engine.rate - 100.0).abs() < f32::EPSILON, "rate borné à 100");
}