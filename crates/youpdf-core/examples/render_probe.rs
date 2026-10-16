//! Sonde de rendu : vérifie dimensions de page et bitmap pdfium.
//! Usage : `cargo run -p youpdf-core --example render_probe -- <fichier.pdf>`

use std::io::Write;

use youpdf_core::{Document, PdfiumRenderer, RenderOptions, Renderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Users/gabrielzeizer/.opencode/plan/youpdf/sample.pdf".to_owned());

    let doc = Document::open(&path)?;
    println!("pages: {}", doc.page_count());
    let (w, h) = doc.page_size(1);
    println!("page_size(1) = {w} x {h} pt");

    let renderer = PdfiumRenderer::load()?;
    for scale in [1.0_f32, 3.0, 4.02, 6.0] {
        let bmp = renderer.render_page(&doc, 1, &RenderOptions { scale, ..Default::default() })?;
        let image = bmp.to_egui_image();
        let (iw, ih) = (image.width(), image.height());
        let ink = image.pixels.iter().filter(|p| p.r() < 200 || p.g() < 200 || p.b() < 200).count();
        println!("scale {scale}: bitmap {iw} x {ih}, ink px {ink}");

        // Écrit un PPM (P6) pour inspection.
        let file = format!("/private/var/folders/rv/m46fplc57jb5v1k82v053hk80000gn/T/opencode/probe-{scale}.ppm");
        let mut out = std::fs::File::create(&file)?;
        write!(out, "P6\n{iw} {ih}\n255\n")?;
        let mut bytes = Vec::with_capacity(iw * ih * 3);
        for px in &image.pixels {
            bytes.push(px.r());
            bytes.push(px.g());
            bytes.push(px.b());
        }
        out.write_all(&bytes)?;
        println!("  -> {file}");
    }
    Ok(())
}
