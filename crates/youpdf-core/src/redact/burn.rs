//! Brûlage de pixels : effacement définitif dans les images.
//!
//! SQUELETTE — l'implémentation réelle est écrite par l'agent dédié.
//! Ne modifiez que ce fichier.

use crate::document::Document;
use crate::redact::geometry::BBox;
use crate::redact::{RedactionFill, RedactionOverlay};

use youpdf_shared::Result;

/// Brûle les zones caviardées dans une image embarquée.
///
/// `page_bbox` est la boîte de l'image sur la page, en repère page.
/// `regions` sont les zones à effacer, également en repère page.
///
/// Renvoie `true` si l'image a été modifiée.
pub(crate) fn burn_image(
    doc: &mut Document,
    page_number: usize,
    resource_name: &str,
    page_bbox: &BBox,
    regions: &[BBox],
    fill: RedactionFill,
    overlay: Option<&RedactionOverlay>,
) -> Result<bool> {
    todo!()
}

/// Remplit un rectangle dans un bitmap RGBA déjà rendu.
///
/// Le rectangle est en coordonnées **affichées** (origine bas-gauche) ; le
/// bitmap est en coordonnées écran (origine haut-gauche).
pub(crate) fn paint_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    display_w: f32,
    display_h: f32,
    rect: [f32; 4],
    fill: RedactionFill,
) {
    todo!()
}

/// Dessine un texte ASCII dans un bitmap RGBA.
///
/// `x`, `y` est le coin bas-gauche du texte, en pixels, origine bas-gauche.
/// `scale` est la hauteur d'une ligne en pixels.
pub(crate) fn draw_text(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    text: &str,
    scale: f32,
    color: [u8; 3],
) {
    todo!()
}

/// Couleur RGB effective d'un remplissage.
pub(crate) fn fill_rgb(fill: RedactionFill) -> [u8; 3] {
    todo!()
}
