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
