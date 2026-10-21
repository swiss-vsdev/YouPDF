//! Chirurgie du flux de contenu : suppression ciblée des instructions.
//!
//! SQUELETTE — l'implémentation réelle est écrite par l'agent dédié.
//! Ne modifiez que ce fichier.

use lopdf::content::Operation;

use crate::document::Document;
use crate::redact::geometry::BBox;

use youpdf_shared::Result;

/// Résultat de l'analyse d'un flux de contenu.
#[derive(Debug, Clone, Default)]
pub struct SliceOutcome {
    /// Instructions conservées, réécrites.
    pub kept: Vec<Operation>,
    /// Instructions de texte supprimées.
    pub text_ops_removed: usize,
    /// Instructions vectorielles supprimées.
    pub vector_ops_removed: usize,
    /// Vrai si le flux ne peut pas être découpé proprement et exige une
    /// rasterisation de la page.
    pub needs_raster: bool,
    /// Images dont la zone recoupe une zone à effacer.
    pub images_touched: Vec<TouchedImage>,
    /// Opérateurs rencontrés que l'interpréteur ne sait pas traiter.
    pub unsupported: Vec<String>,
}

/// Une image dont la boîte de placement recoupe une zone à effacer.
#[derive(Debug, Clone)]
pub struct TouchedImage {
    /// Nom de ressource, sans le `/` initial.
    pub resource_name: String,
    /// Boîte de placement dans le repère page.
    pub bbox: BBox,
}

/// Analyse les flux de contenu d'une page et retire ce qui tombe dans les
/// zones.
///
/// `regions` sont les zones à effacer, en repère **page**.
///
/// Si le contenu de la page ne peut pas être décodé (image inline, flux
/// corrompu) ou contient un opérateur qui peut peindre sans être
/// interprétable, `needs_raster` passe à `true` : la page devra être
/// rasterisée.
pub(crate) fn slice_page(
    doc: &Document,
    page_id: lopdf::ObjectId,
    regions: &[BBox],
) -> Result<SliceOutcome> {
    todo!()
}

/// Réécrit le flux de contenu d'une page.
///
/// `kept` est la suite d'instructions à écrire. Si elle est vide, le flux
/// est vidé.
pub(crate) fn write_back(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    kept: &[Operation],
) -> Result<()> {
    todo!()
}

/// Remplace le tableau `/Annots` d'une page.
pub(crate) fn set_annots(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    kept: &[lopdf::ObjectId],
) -> Result<()> {
    todo!()
}

/// Opérateurs que l'interpréteur sait traiter.
pub(crate) fn is_known_operator(operator: &str) -> bool {
    todo!()
}
