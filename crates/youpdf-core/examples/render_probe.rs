//! Sonde de rendu : vérifie dimensions de page et bitmap pdfium.
//! Usage : `cargo run -p youpdf-core --example render_probe -- <fichier.pdf> [échelle]`
//!
//! À l'échelle demandée (1.0 par défaut), imprime pour chaque page la taille
//! d'affichage annoncée par le document et celle du bitmap produit par
//! pdfium : les deux doivent coïncider, y compris sur une page tournée avec
//! `/Rotate`.

use youpdf_core::{Document, PdfiumRenderer, RenderOptions, Renderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Users/gabrielzeizer/.opencode/plan/youpdf/sample.pdf".to_owned());
    let scale: f32 = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.0);

    let doc = Document::open(&path)?;
    println!("pages: {}", doc.page_count());

    let renderer = PdfiumRenderer::load()?;
    for page in 1..=doc.page_count() as u32 {
        let (w, h) = doc.page_size(page);
        let bmp =
            renderer.render_page(&doc, page, &RenderOptions { scale, ..Default::default() })?;
        let image = bmp.to_egui_image();
        let ink = image
            .pixels
            .iter()
            .filter(|p| p.r() < 200 || p.g() < 200 || p.b() < 200)
            .count();
        println!(
            "page {page}: annoncée {w:.1} x {h:.1} pt · bitmap {} x {} px (échelle {scale}) · encre {ink} px",
            image.width(),
            image.height()
        );
    }
    Ok(())
}
