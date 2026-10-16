//! Conversion PDF → autres formats.
//!
//! ## État actuel
//!
//! Seul le format **DOCX** (Word) est implémenté, avec une approche
//! simple : extraction du texte de chaque page et génération d'un
//! document Word où chaque page PDF devient une section avec un saut
//! de page.
//!
//! Ce n'est **pas** une conversion haute fidélité : les images, les
//! tableaux et la mise en page avancée ne sont pas préservés. C'est
//! annoncé honnêtement dans l'interface.
//!
//! ## Extensibilité
//!
//! Le trait [`Converter`] est volontairement minimal pour faciliter
//! l'ajout d'autres formats (HTML, CSV, Markdown…).

/// ponytail: `docx-rs` est désactivé par défaut — sa chaîne de dépendances
/// pointe vers des versions `zip` retirées de crates.io. Activer la feature
/// `convert-docx` quand l'amont sera réparé.
#[cfg(feature = "convert-docx")]
pub mod docx;

use std::path::Path;

use youpdf_shared::Result;

use crate::document::Document;

/// Résultat d'une conversion.
#[derive(Debug, Clone)]
pub struct ConversionReport {
    /// Nombre de pages traitées.
    pub pages: usize,
    /// Nombre de caractères convertis.
    pub chars: usize,
    /// Avertissements non bloquants (images ignorées, etc.).
    pub warnings: Vec<String>,
}

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
}

impl TextFormat {
    /// Nom affiché dans l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            TextFormat::Plain => "Plain text (.txt)",
            TextFormat::Markdown => "Markdown (.md)",
            TextFormat::Html => "Web page (.html)",
        }
    }

    /// Extension de fichier suggérée (sans point).
    pub fn extension(&self) -> &'static str {
        match self {
            TextFormat::Plain => "txt",
            TextFormat::Markdown => "md",
            TextFormat::Html => "html",
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
        }
        Ok(out)
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

/// Interface d'un convertisseur.
pub trait Converter {
    /// Nom lisible du format de sortie (affiché dans les menus).
    fn display_name(&self) -> &str;

    /// Extension de fichier suggérée (sans point).
    fn extension(&self) -> &str;

    /// Convertit un document PDF et écrit le résultat dans `output`.
    fn convert(&self, doc: &Document, output: &Path) -> Result<ConversionReport>;
}
