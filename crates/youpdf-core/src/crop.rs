//! Recadrage des pages (`/CropBox`).
//!
//! Le recadrage retire des marges de chaque bord de la boîte visible et écrit
//! le résultat dans le `/CropBox` de la page. La rotation n'entre pas en jeu :
//! `/CropBox` vit dans le repère non tourné de la page, comme les champs de
//! [`crate::document::PageView`].

use lopdf::Object;
use youpdf_shared::Result;

use crate::document::Document;

/// Marges de recadrage, en points, retirées de chaque bord de la page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropMargins {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Default for CropMargins {
    fn default() -> Self {
        Self {
            left: 18.0,
            bottom: 18.0,
            right: 18.0,
            top: 18.0,
        }
    }
}

/// Ramène une paire de marges opposées pour laisser au moins `1.0` pt.
///
/// Une marge négative agrandit la boîte : elle n'a jamais besoin d'être
/// réduite. On rogne donc d'abord la marge « loin » (droite ou haut), puis la
/// marge « près » (gauche ou bas) si le dépassement persiste.
fn clamp_margins(near: f32, far: f32, span: f32) -> (f32, f32) {
    if span - near - far >= 1.0 {
        return (near, far);
    }
    let mut excess = 1.0 - (span - near - far);
    let cut_far = excess.min(far.max(0.0));
    let far = far - cut_far;
    excess -= cut_far;
    let cut_near = excess.min(near.max(0.0));
    (near - cut_near, far)
}

/// Lit quatre nombres dans un tableau d'objets PDF.
fn array_box(array: &[Object]) -> Option<[f32; 4]> {
    if array.len() < 4 {
        return None;
    }
    let num = |i: usize| -> Option<f32> {
        array[i]
            .as_float()
            .ok()
            .or_else(|| array[i].as_i64().ok().map(|v| v as f32))
    };
    Some([num(0)?, num(1)?, num(2)?, num(3)?])
}

impl Document {
    /// Recadre les pages indiquées en retirant `margins` de chaque bord.
    ///
    /// Les pages sont numérotées à partir de 1. La boîte de départ est la
    /// boîte visible courante (`/CropBox` existant, sinon `/MediaBox`). Un
    /// résultat de largeur ou de hauteur inférieur à `1.0` pt est ramené à
    /// `1.0` pt en rognant la marge fautive, jamais en inversant la boîte.
    /// Une marge négative agrandit la boîte. Renvoie le nombre de pages
    /// effectivement modifiées.
    pub fn crop_pages(&mut self, pages: &[usize], margins: CropMargins) -> Result<usize> {
        let mut changed = 0usize;
        for number in pages {
            let page = *number as u32;
            let Ok(id) = self.page_id(page) else {
                continue;
            };
            let view = self.page_view(page);
            let (left, right) = clamp_margins(margins.left, margins.right, view.width);
            let (bottom, top) = clamp_margins(margins.bottom, margins.top, view.height);
            let Ok(dict) = self.inner.get_dictionary_mut(id) else {
                continue;
            };
            dict.set(
                "CropBox",
                Object::Array(vec![
                    Object::Real(view.x + left),
                    Object::Real(view.y + bottom),
                    Object::Real(view.x + view.width - right),
                    Object::Real(view.y + view.height - top),
                ]),
            );
            changed += 1;
        }
        if changed > 0 {
            self.touch();
        }
        Ok(changed)
    }

    /// Retire le recadrage des pages indiquées.
    ///
    /// `/CropBox` est remis à la valeur du `/MediaBox` résolu en remontant la
    /// chaîne des `/Parent`. Si aucun `/MediaBox` n'existe, la boîte visible
    /// courante sert de repli. Renvoie le nombre de pages modifiées.
    pub fn reset_crop(&mut self, pages: &[usize]) -> Result<usize> {
        let mut changed = 0usize;
        for number in pages {
            let page = *number as u32;
            let Ok(id) = self.page_id(page) else {
                continue;
            };
            let target = match self.resolved_media_box(page) {
                Some(box_) => box_,
                None => {
                    let view = self.page_view(page);
                    [
                        view.x,
                        view.y,
                        view.x + view.width,
                        view.y + view.height,
                    ]
                }
            };
            let Ok(dict) = self.inner.get_dictionary_mut(id) else {
                continue;
            };
            dict.set(
                "CropBox",
                Object::Array(vec![
                    Object::Real(target[0]),
                    Object::Real(target[1]),
                    Object::Real(target[2]),
                    Object::Real(target[3]),
                ]),
            );
            changed += 1;
        }
        if changed > 0 {
            self.touch();
        }
        Ok(changed)
    }

    /// Boîte visible (`/CropBox`) d'une page, si elle en a une propre.
    ///
    /// Seul le `/CropBox` porté par la page elle-même est lu : une valeur
    /// héritée du nœud `Pages` n'est pas renvoyée. Format `[x1, y1, x2, y2]`.
    pub fn crop_box(&self, page: u32) -> Option<[f32; 4]> {
        let id = self.page_id(page).ok()?;
        let dict = self.inner.get_dictionary(id).ok()?;
        let object = dict.get(b"CropBox").ok()?;
        let array = object.as_array().ok()?;
        array_box(array)
    }

    /// `/MediaBox` résolu d'une page, en remontant les `/Parent`.
    fn resolved_media_box(&self, page: u32) -> Option<[f32; 4]> {
        let mut current = Some(self.page_id(page).ok()?);
        for _ in 0..16 {
            let Some(id) = current else { break };
            let Ok(dict) = self.inner.get_dictionary(id) else {
                break;
            };
            if let Ok(object) = dict.get_deref(b"MediaBox", &self.inner) {
                if let Ok(array) = object.as_array() {
                    if let Some(box_) = array_box(array) {
                        if (box_[2] - box_[0]).abs() > 0.0 && (box_[3] - box_[1]).abs() > 0.0 {
                            return Some(box_);
                        }
                    }
                }
            }
            current = dict
                .get(b"Parent")
                .ok()
                .and_then(|parent| parent.as_reference().ok());
        }
        None
    }
}
