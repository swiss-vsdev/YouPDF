//! Panel de lecture : mise en page, zoom, pan, recherche.

use super::*;
use youpdf_core::geom::rect_to_display;

/// Extrait et met en cache les blocs de texte d'une page, en repère affiché.
///
/// Répond `false` si le moteur n'a rien lu : l'appelant ne doit alors pas
/// proposer de sélection sur cette page.
pub fn extract_page_fragments(renderer: &dyn Renderer, tab: &mut Tab, page: u32) -> bool {
    match renderer.page_fragments(&tab.doc, page) {
        Ok(mut frags) => {
            for frag in &mut frags {
                frag.rect = rect_to_display(&tab.doc, page as usize, frag.rect);
                // Les boîtes de glyphes vivent dans le même repère que le
                // fragment : sans cette conversion, la sélection au caractère
                // serait décalée sur une page tournée ou recadrée.
                for item in &mut frag.chars {
                    item.rect = rect_to_display(&tab.doc, page as usize, item.rect);
                }
            }
            let usable = !frags.is_empty();
            tab.fragments.insert(page, frags);
            usable
        }
        Err(err) => {
            tracing::warn!(%err, page, "extraction du texte de la page");
            tab.fragments.insert(page, Vec::new());
            false
        }
    }
}

/// Palier de vignette : hors de la pyramide de zoom, une vignette ne doit
/// jamais être servie à la place d'une page rendue.
pub const THUMBNAIL_STEP: i32 = -1;

/// Délai de repos après le dernier changement de zoom avant de rastériser la
/// page à la finesse demandée.
pub const ZOOM_SETTLE_SECS: f64 = 0.12;

/// Délai de repos après le dernier défilement avant d'affiner les pages
/// rastérisées à basse résolution.
pub const SCROLL_SETTLE_SECS: f64 = 0.12;

/// Échelle de rastérisation pendant un geste de défilement. Le coût de
/// PDFium est quadratique : à 0,4× l'image pèse 16 % de la résolution
/// complète, ce qui allège la frame d'un ordre de grandeur. Elle est
/// étirée puis affinée à l'arrêt du geste.
pub const FAST_RENDER_SCALE: f32 = 0.4;

/// Réserve en haut du canevas pour les barres flottantes (outils, modes) :
/// sans elle, la première page passe sous la barre d'outils. La barre de modes
/// mesure 34 pt et commence 8 pt sous la barre de titre.
pub const CANVAS_TOP_GAP: f32 = 48.0;