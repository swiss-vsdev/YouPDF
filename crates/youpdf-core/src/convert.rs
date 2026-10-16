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

/// Interface d'un convertisseur.
pub trait Converter {
    /// Nom lisible du format de sortie (affiché dans les menus).
    fn display_name(&self) -> &str;

    /// Extension de fichier suggérée (sans point).
    fn extension(&self) -> &str;

    /// Convertit un document PDF et écrit le résultat dans `output`.
    fn convert(&self, doc: &Document, output: &Path) -> Result<ConversionReport>;
}
