//! Conversion de coordonnées entre repère affiché et repère page, et recherche
//! de lien sous un point.

use crate::document::Document;
use crate::links::PageLink;
use crate::annotate::Annotation;

/// Recadre les annotations du repère affiché (celui du canevas) vers le repère
/// page, rotation et boîte visible comprises.
///
/// L'interface dessine et sélectionne en repère affiché ; le PDF s'écrit en
/// repère page. Sans cette conversion, une annotation posée sur une page
/// tournée ou recadrée atterrit au mauvais endroit dans le fichier.
pub fn annots_to_page(doc: &Document, annots: &[Annotation]) -> Vec<Annotation> {
    let mut out = annots.to_vec();
    for ann in &mut out {
        let view = doc.page_view(ann.page as u32);
        ann.rect = view.page_rect(ann.rect);
        for stroke in &mut ann.ink_points {
            for point in stroke.iter_mut() {
                *point = view.page_point(*point);
            }
        }
        for quad in &mut ann.quads {
            for point in quad.chunks_exact_mut(2) {
                let converted = view.page_point([point[0], point[1]]);
                point[0] = converted[0];
                point[1] = converted[1];
            }
        }
    }
    out
}

/// L'inverse : les annotations lues dans le PDF reviennent en repère affiché.
pub fn annots_to_display(doc: &Document, mut annots: Vec<Annotation>) -> Vec<Annotation> {
    for ann in &mut annots {
        let view = doc.page_view(ann.page as u32);
        ann.rect = view.display_rect(ann.rect);
        for stroke in &mut ann.ink_points {
            for point in stroke.iter_mut() {
                *point = view.display_point(*point);
            }
        }
        for quad in &mut ann.quads {
            for point in quad.chunks_exact_mut(2) {
                let converted = view.display_point([point[0], point[1]]);
                point[0] = converted[0];
                point[1] = converted[1];
            }
        }
    }
    annots
}

/// Lien sous un point, en repère page.
///
/// La tolérance élargit la boîte du lien : un lien dessiné au pixel près
/// est difficile à toucher, et le lecteur pardonne le voisinage. Le
/// dernier lien trouvé l'emporte, comme l'annotation du dessus dans
/// `hit_test`.
pub fn link_at(links: &[PageLink], point: [f32; 2]) -> Option<&PageLink> {
    const TOLERANCE: f32 = 3.0;
    links
        .iter()
        .rev()
        .find(|link| {
            point[0] >= link.rect[0] - TOLERANCE
                && point[0] <= link.rect[2] + TOLERANCE
                && point[1] >= link.rect[1] - TOLERANCE
                && point[1] <= link.rect[3] + TOLERANCE
        })
}