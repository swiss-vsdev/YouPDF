//! Sonde : pipeline de recherche comme l'application (index via pdfium).
use youpdf_core::{Document, PdfiumRenderer, Renderer, SearchIndex, SearchOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap();
    let query = std::env::args().nth(2).unwrap_or_else(|| "lorem".to_owned());
    let doc = Document::open(&path)?;
    let renderer = PdfiumRenderer::load()?;
    let index = SearchIndex::build_with(&doc, |n| {
        renderer.engine_page_text(&doc, n)
            .filter(|t| !t.trim().is_empty())
            .or_else(|| doc.page_text(n).ok())
            .unwrap_or_default()
    })?;
    let hits = index.search_with(&query, 50, SearchOptions::default());
    println!("requête {query:?} : {} correspondance(s)", hits.len());
    for h in hits.iter().take(5) { println!("  page {} · {:?}", h.page, h.context.trim()); }
    Ok(())
}
