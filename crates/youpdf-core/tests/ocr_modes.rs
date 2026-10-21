//! Modes de sortie OCR : calque invisible, texte visible, page sans scan.

use youpdf_core::ocr::{make_ocr, OcrEngine, OcrMode, OcrResult, OcrWord};
use youpdf_core::render::PageBitmap;
use youpdf_core::{blank_document, Document, PdfiumRenderer};

struct FakeEngine;

impl OcrEngine for FakeEngine {
    fn display_name(&self) -> &str {
        "fake"
    }

    fn recognize(
        &self,
        _bitmap: &PageBitmap,
        page: usize,
        language: &str,
        mode: OcrMode,
    ) -> youpdf_shared::Result<OcrResult> {
        Ok(OcrResult {
            page,
            text: "Bonjour".to_string(),
            language: language.to_string(),
            mode,
            char_count: 7,
            words: vec![OcrWord {
                left: 10,
                top: 10,
                width: 80,
                height: 20,
                confidence: 95,
                text: "Bonjour".to_string(),
            }],
        })
    }
}

fn contenu(doc: &Document) -> String {
    let bytes = doc.to_bytes().expect("sérialisation");
    let raw = lopdf::Document::load_mem(&bytes).expect("relecture");
    let page = raw.get_pages().values().copied().next().expect("page");
    String::from_utf8_lossy(&raw.get_page_content(page).expect("contenu")).into_owned()
}

fn fixture() -> Option<(Document, PdfiumRenderer)> {
    let renderer = PdfiumRenderer::load().ok()?;
    let doc = blank_document(1, 300.0, 400.0).ok()?;
    Some((doc, renderer))
}

#[test]
fn modes_choisissent_la_visibilite_du_texte() {
    let Some((source, renderer)) = fixture() else {
        return;
    };
    let engine = FakeEngine;

    let searchable = make_ocr(&source, &renderer, &engine, "eng", OcrMode::Searchable)
        .expect("mode consultable");
    let text = contenu(&searchable);
    assert!(text.contains("3 Tr"), "texte invisible attendu : {text}");
    assert!(text.contains("/Im0 Do"), "image de scan attendue");

    let text_image = make_ocr(&source, &renderer, &engine, "eng", OcrMode::TextAndImage)
        .expect("mode texte et image");
    let text = contenu(&text_image);
    assert!(text.contains("0 Tr"), "texte visible attendu : {text}");
    assert!(text.contains("/Im0 Do"), "image de scan attendue");

    let editable = make_ocr(&source, &renderer, &engine, "eng", OcrMode::Editable)
        .expect("mode modifiable");
    let text = contenu(&editable);
    assert!(!text.contains("/Im0 Do"), "aucune image de scan : {text}");
    assert!(text.contains("0 Tr"), "texte visible attendu : {text}");
}

#[test]
fn ocr_requires_real_renderer() {
    use youpdf_core::render::NullRenderer;
    let doc = blank_document(1, 300.0, 400.0).expect("document");
    let engine = FakeEngine;
    let result = make_ocr(&doc, &NullRenderer, &engine, "eng", OcrMode::Searchable);
    assert!(result.is_err(), "un rendu vide doit être refusé");
}
