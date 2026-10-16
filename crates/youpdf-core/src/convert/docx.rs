//! Conversion PDF → DOCX via `docx-rs`.
//!
//! ## Ce qui est préservé
//!
//! - Le texte page par page
//! - Les sauts de ligne (traduits en paragraphes Word)
//! - Les sauts de page (une section Word par page PDF)
//!
//! ## Ce qui n'est **pas** préservé
//!
//! - Les images
//! - Les tableaux
//! - La mise en page (colonnes, marges exactes, polices originales)
//! - Les annotations
//!
//! Le rapport de conversion liste les avertissements correspondants.

use std::path::Path;

use docx_rs::*;

use youpdf_shared::{Error, Result};

use super::{ConversionReport, Converter};
use crate::document::Document;

/// Convertisseur PDF → DOCX.
pub struct DocxConverter;

impl Converter for DocxConverter {
    fn display_name(&self) -> &str {
        "Document Word (.docx)"
    }

    fn extension(&self) -> &str {
        "docx"
    }

    fn convert(&self, doc: &Document, output: &Path) -> Result<ConversionReport> {
        let page_count = doc.page_count();
        if page_count == 0 {
            return Err(Error::InvalidInput("document vide".into()));
        }

        let mut docx = Docx::new();
        let mut total_chars = 0usize;
        let mut pages_without_text = 0usize;

        for page_n in 1..=page_count {
            // Titre de section (informatif, discret).
            docx = docx.add_paragraph(
                Paragraph::new().add_run(
                    Run::new()
                        .add_text(format!("— Page {page_n} —"))
                        .color("999999")
                        .size(18),
                ),
            );

            let text = match doc.page_text(page_n as u32) {
                Ok(t) => t,
                Err(e) => {
                    tracing::debug!(page = page_n, error = %e, "extraction impossible");
                    pages_without_text += 1;
                    docx = docx.add_paragraph(
                        Paragraph::new().add_run(
                            Run::new()
                                .add_text("[texte non extractible — page probablement scannée]")
                                .italic()
                                .color("999999"),
                        ),
                    );
                    if page_n < page_count {
                        docx = docx.add_paragraph(Paragraph::new().page_break_before(true));
                    }
                    continue;
                }
            };

            if text.trim().is_empty() {
                pages_without_text += 1;
            }

            // Découpage par lignes : chaque ligne devient un paragraphe.
            // Les lignes vides successives sont fusionnées en un seul saut.
            for paragraph in split_paragraphs(&text) {
                total_chars += paragraph.len();
                docx =
                    docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(&paragraph)));
            }

            // Saut de page explicite avant la page suivante.
            if page_n < page_count {
                docx = docx.add_paragraph(Paragraph::new().page_break_before(true));
            }
        }

        // Écriture du fichier .docx.
        let file = std::fs::File::create(output).map_err(Error::Io)?;
        docx.build()
            .pack(file)
            .map_err(|e| Error::Internal(format!("écriture DOCX : {e}")))?;

        let mut warnings = Vec::new();
        if pages_without_text > 0 {
            warnings.push(format!(
                "{pages_without_text} page(s) sans texte extractible — utilisez l'OCR pour les convertir."
            ));
        }
        warnings.push("Les images et tableaux ne sont pas préservés dans cette version.".into());

        tracing::info!(
            pages = page_count,
            chars = total_chars,
            output = %output.display(),
            "conversion DOCX terminée"
        );

        Ok(ConversionReport {
            pages: page_count,
            chars: total_chars,
            warnings,
        })
    }
}

/// Découpe un texte en paragraphes.
///
/// On considère qu'une ligne vide (ou plusieurs) marque une frontière
/// de paragraphe. Les lignes consécutives non vides sont jointes par
/// un espace, sauf si elles ressemblent à des puces.
fn split_paragraphs(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }

        // Détection simple de puces : commence par -, •, *, ou 1. / 1)
        let looks_like_bullet = trimmed.starts_with("•")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || (trimmed
                .split_once(['.', ')'])
                .map(|(prefix, _)| prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty())
                .unwrap_or(false));

        if looks_like_bullet && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(trimmed);
    }

    if !current.is_empty() {
        out.push(current);
    }

    if out.is_empty() {
        out.push(String::new());
    }
    out
}
