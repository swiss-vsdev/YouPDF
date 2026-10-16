//! Sonde de texte : imprime le texte extrait de chaque page et les
//! correspondances d'une requête.
//!
//! Usage : `cargo run -p youpdf-core --example text_probe -- <fichier.pdf> [requête]`

use youpdf_core::{Document, SearchIndex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Users/gabrielzeizer/.opencode/plan/youpdf/sample.pdf".to_owned());
    let query = std::env::args().nth(2);

    let doc = Document::open(&path)?;
    println!("pages: {}", doc.page_count());

    let index = SearchIndex::build(&doc)?;
    for page in 1..=doc.page_count() {
        let text = doc.page_text(page as u32).unwrap_or_default();
        println!("-- page {page} ({} car.) : {}", text.chars().count(), text.trim());
    }
    if let Some(query) = query {
        let hits = index.search(&query, 50);
        println!("requête {query:?} : {} correspondance(s)", hits.len());
        for hit in hits.iter().take(10) {
            println!("   page {} · {}", hit.page, hit.context);
        }
    }
    Ok(())
}
