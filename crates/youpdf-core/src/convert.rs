//! Conversion PDF → autres formats.
//!
//! ## État actuel
//!
//! La couche texte (`.txt`, `.md`, `.html`, `.csv`, `.xml`, `.rtf`) et
//! l'export d'une image par page (`.png`, `.jpg`). La reconstruction
//! bureautique haut de gamme (Word, Excel, PowerPoint) vit dans
//! [`crate::office`].

use std::path::{Path, PathBuf};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::render::{PdfiumRenderer, RenderOptions, Renderer};
use crate::util::{csv_escape, escape_xml};

/// Format texte proposé à l'export.
///
/// L'export ne transporte que la couche texte : les images, les tableaux et
/// la mise en page du PDF ne sont pas reconstruits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    /// Texte brut, pages séparées par une ligne vide.
    Plain,
    /// Markdown, une section « ## Page N » par page.
    Markdown,
    /// Page web autonome, une section par page.
    Html,
    /// Tableur CSV, une ligne par ligne de texte (`page,line,text`).
    Csv,
    /// XML, une `<page>` par page contenant une `<line>` par ligne.
    Xml,
    /// Rich Text Format, un paragraphe par ligne.
    Rtf,
}

impl TextFormat {
    /// Nom affiché dans l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            TextFormat::Plain => "Plain text (.txt)",
            TextFormat::Markdown => "Markdown (.md)",
            TextFormat::Html => "Web page (.html)",
            TextFormat::Csv => "Comma-separated values (.csv)",
            TextFormat::Xml => "XML (.xml)",
            TextFormat::Rtf => "Rich Text Format (.rtf)",
        }
    }

    /// Extension de fichier suggérée (sans point).
    pub fn extension(&self) -> &'static str {
        match self {
            TextFormat::Plain => "txt",
            TextFormat::Markdown => "md",
            TextFormat::Html => "html",
            TextFormat::Csv => "csv",
            TextFormat::Xml => "xml",
            TextFormat::Rtf => "rtf",
        }
    }
}

/// Format d'image proposé à l'export des pages.
///
/// Chaque page est rendue en bitmap par pdfium puis écrite sur disque. C'est
/// une conversion fidèle au rendu : le texte n'est pas sélectionnable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG, sans perte, conserve la transparence.
    Png,
    /// JPEG, avec perte, adapté aux documents photographiques.
    Jpeg,
    /// TIFF, sans perte, pour l'archivage et l'impression.
    Tiff,
}

impl ImageFormat {
    /// Nom affiché dans l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            ImageFormat::Png => "Image PNG (.png)",
            ImageFormat::Jpeg => "Image JPEG (.jpg)",
            ImageFormat::Tiff => "Image TIFF (.tiff)",
        }
    }

    /// Extension de fichier suggérée (sans point).
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Tiff => "tiff",
        }
    }
}

impl Document {
    /// Exporte la couche texte du document dans le format demandé.
    ///
    /// Les pages sans texte extractible (scan non océrisé) donnent une
    /// section vide plutôt qu'une erreur : un document mixte s'exporte
    /// quand même.
    pub fn export_text(&self, format: TextFormat) -> Result<String> {
        let total = self.page_count() as u32;
        let title = self
            .metadata()
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| self.display_name());

        let mut pages: Vec<String> = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let text = self.page_text(number).unwrap_or_default();
            let text = text.replace('\u{c}', "\n").trim().to_string();
            pages.push(text);
        }

        let mut out = String::new();
        match format {
            TextFormat::Plain => {
                for (index, page) in pages.iter().enumerate() {
                    if index > 0 {
                        out.push_str("\n\n");
                    }
                    out.push_str(page);
                }
                out.push('\n');
            }
            TextFormat::Markdown => {
                out.push_str(&format!("# {}\n", escape_markdown(&title)));
                for (index, page) in pages.iter().enumerate() {
                    out.push_str(&format!("\n## Page {}\n\n", index + 1));
                    out.push_str(page);
                    out.push('\n');
                }
            }
            TextFormat::Html => {
                out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
                out.push_str("<meta charset=\"utf-8\">\n");
                out.push_str(&format!("<title>{}</title>\n", escape_html(&title)));
                out.push_str("</head>\n<body>\n");
                for (index, page) in pages.iter().enumerate() {
                    out.push_str(&format!(
                        "<section class=\"page\" id=\"page-{}\">\n",
                        index + 1
                    ));
                    for paragraph in page.split("\n\n") {
                        let paragraph = paragraph.trim();
                        if paragraph.is_empty() {
                            continue;
                        }
                        let html = escape_html(paragraph).replace('\n', "<br>\n");
                        out.push_str(&format!("<p>{html}</p>\n"));
                    }
                    out.push_str("</section>\n");
                }
                out.push_str("</body>\n</html>\n");
            }
            TextFormat::Csv => {
                out.push_str("page,line,text\n");
                for (page_index, page) in pages.iter().enumerate() {
                    let page_number = page_index + 1;
                    for (line_index, line) in page.split('\n').enumerate() {
                        if line.is_empty() {
                            continue;
                        }
                        out.push_str(&csv_escape(&page_number.to_string()));
                        out.push(',');
                        out.push_str(&csv_escape(&(line_index + 1).to_string()));
                        out.push(',');
                        out.push_str(&csv_escape(line));
                        out.push('\n');
                    }
                }
            }
            TextFormat::Xml => {
                out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
                out.push_str("<document>\n");
                for (page_index, page) in pages.iter().enumerate() {
                    out.push_str(&format!(
                        "  <page number=\"{}\">\n",
                        page_index + 1
                    ));
                    for line in page.split('\n') {
                        out.push_str(&format!("    <line>{}</line>\n", escape_xml(line)));
                    }
                    out.push_str("  </page>\n");
                }
                out.push_str("</document>\n");
            }
            TextFormat::Rtf => {
                out.push_str("{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Helvetica;}}");
                for (page_index, page) in pages.iter().enumerate() {
                    if page_index > 0 {
                        out.push_str("\\page\n");
                    }
                    for line in page.split('\n') {
                        out.push_str(&escape_rtf(line));
                        out.push_str("\\par\n");
                    }
                }
                out.push('}');
            }
        }
        Ok(out)
    }

    /// Exporte chaque page en image, dans `dir`.
    ///
    /// `dpi` fixe la résolution du rendu (bornée à 36 DPI minimum). Les
    /// fichiers sont nommés `page-0001.png` / `page-0001.jpg` (numéro de page
    /// sur quatre chiffres, 1-indexé) et renvoyés dans l'ordre des pages.
    ///
    /// Nécessite la bibliothèque pdfium (voir [`PdfiumRenderer::load`]).
    pub fn export_images(
        &self,
        dir: &Path,
        format: ImageFormat,
        dpi: f32,
    ) -> Result<Vec<PathBuf>> {
        std::fs::create_dir_all(dir)?;

        let renderer = PdfiumRenderer::load()?;
        let options = RenderOptions {
            scale: dpi.max(36.0) / 72.0,
            ..Default::default()
        };

        let total = self.page_count() as u32;
        let mut paths = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let bitmap = renderer.render_page(self, number, &options)?;
            let path = dir.join(format!("page-{number:04}.{}", format.extension()));

            match format {
                ImageFormat::Png => {
                    let image =
                        image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.pixels)
                            .ok_or_else(|| {
                                Error::Pdf(format!(
                                    "tampon RGBA invalide pour la page {number}"
                                ))
                            })?;
                    image.save(&path).map_err(|e| {
                        Error::Pdf(format!("écriture PNG {} : {e}", path.display()))
                    })?;
                }
                ImageFormat::Tiff => {
                    let image =
                        image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.pixels)
                            .ok_or_else(|| {
                                Error::Pdf(format!(
                                    "tampon RGBA invalide pour la page {number}"
                                ))
                            })?;
                    image.save(&path).map_err(|e| {
                        Error::Pdf(format!("écriture TIFF {} : {e}", path.display()))
                    })?;
                }
                ImageFormat::Jpeg => {
                    // JPEG ne transporte pas de canal alpha : on l'abandonne.
                    let mut rgb = Vec::with_capacity(bitmap.width as usize * bitmap.height as usize * 3);
                    for chunk in bitmap.pixels.chunks_exact(4) {
                        rgb.extend_from_slice(&chunk[..3]);
                    }
                    let image = image::RgbImage::from_raw(bitmap.width, bitmap.height, rgb)
                        .ok_or_else(|| {
                            Error::Pdf(format!("tampon RGB invalide pour la page {number}"))
                        })?;
                    image.save(&path).map_err(|e| {
                        Error::Pdf(format!("écriture JPEG {} : {e}", path.display()))
                    })?;
                }
            }

            paths.push(path);
        }
        Ok(paths)
    }
}

/// Échappe un texte pour HTML.
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Nettoie un titre pour une ligne Markdown.
fn escape_markdown(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_string()
}

/// Échappe un texte pour RTF.
///
/// `\`, `{` et `}` sont réservés. Tout caractère non ASCII devient une ou
/// deux échappements `\uN?` (N = unité UTF-16 signée ; les caractères hors
/// BMP donnent une paire de substitution, donc deux `\uN?`).
fn escape_rtf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            _ if c.is_ascii() => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}


impl Document {
    /// Empile toutes les pages dans une seule image verticale.
    ///
    /// Utile pour lire un document sur téléphone : une seule bande, sans
    /// pagination. Le rendu passe par pdfium à la résolution demandée.
    pub fn export_long_image(&self, path: &Path, dpi: f32) -> Result<()> {
        let renderer = PdfiumRenderer::load()?;
        let options = RenderOptions {
            scale: dpi.clamp(36.0, 600.0) / 72.0,
            ..Default::default()
        };
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            pages.push(renderer.render_page(self, number, &options)?);
        }
        const GAP: u32 = 16;
        let width = pages.iter().map(|page| page.width).max().unwrap_or(1);
        let height_u64 = pages.iter().map(|page| page.height as u64).sum::<u64>()
            + GAP as u64 * pages.len().saturating_sub(1) as u64;
        if height_u64 > 65_000 {
            return Err(Error::InvalidInput(
                "image trop haute : réduisez la résolution ou la sélection de pages".into(),
            ));
        }
        let mut canvas = image::RgbaImage::from_pixel(width, height_u64 as u32, image::Rgba([255, 255, 255, 255]));
        let mut top = 0u32;
        for page in &pages {
            for row in 0..page.height {
                for column in 0..page.width {
                    let index = ((row * page.width + column) * 4) as usize;
                    let pixel = &page.pixels[index..index + 4];
                    canvas.put_pixel(column, top + row, image::Rgba([pixel[0], pixel[1], pixel[2], pixel[3]]));
                }
            }
            top += page.height + GAP;
        }
        canvas.save(path).map_err(|e| Error::Pdf(format!("écriture {} : {e}", path.display())))?;
        Ok(())
    }
}
