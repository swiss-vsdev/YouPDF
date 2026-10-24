//! Rendu de pages PDF en bitmap, via **pdfium**.
//!
//! pdfium est la bibliothèque de rendu utilisée par Chrome pour afficher
//! les PDF. C'est le standard de fait : fidélité de rendu excellente,
//! performance élevée, licence BSD.
//!
//! Le crate `pdfium-render` fournit les bindings Rust. La bibliothèque
//! dynamique (`libpdfium.so` / `pdfium.dll` / `libpdfium.dylib`) doit être
//! présente à côté du binaire ou dans le `PATH`. Voir le README pour
//! l'installation.

#[cfg(test)]
use std::cell::Cell;

use pdfium_render::prelude::*;

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::TextFragment;

/// Options de rendu d'une page.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Facteur d'échelle (1.0 = 72 DPI, 2.0 = 144 DPI, etc.).
    pub scale: f32,
    /// Rotation appliquée à la page, en degrés (0, 90, 180, 270).
    pub rotation: i32,
    /// Activer l'anticrénelage.
    pub antialias: bool,
    /// Rendre avec fond transparent au lieu de blanc.
    pub transparent: bool,
    /// Intégrer les annotations (surlignages, tampons, signatures
    /// plates, champs de formulaire) dans l'image produite.
    ///
    /// `true` pour l'affichage et l'OCR : c'est ce que l'utilisateur
    /// voit. `false` pour le caviardage, qui doit conserver les
    /// annotations **en vectoriel** à côté de l'image rasterisée — les
    /// incruster puis supprimer `/Annots` les figerait à jamais,
    /// empechant de les déplacer, éditer ou **supprimer**.
    pub render_annotations: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation: 0,
            antialias: true,
            transparent: false,
            render_annotations: true,
        }
    }
}

/// Image RGBA en mémoire, prête à être affichée par egui.
#[derive(Debug, Clone)]
pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    /// Données RGBA8, ligne par ligne, longueur = width * height * 4.
    pub pixels: Vec<u8>,
}

impl PageBitmap {
    /// Crée un bitmap rempli d'une couleur unie.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        // Calcul en `usize` : `width * height * 4` en `u32` déborde pour une
        // page de grande taille et donne un tampon tronqué.
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Couleur de fond autour d'un rectangle : echantillonne six points juste
    /// exterieur au bloc (deux rangees, trois positions horizontales) et prend
    /// la mediane par canal. `None` si aucun point ne tombe dans le bitmap.
    ///
    /// `rect` est en repere affiche, `scale` le rapport pixels/point du bitmap.
    pub fn background_color_around(&self, scale: f32, rect: [f32; 4]) -> Option<[u8; 3]> {
        if scale <= 0.0 || self.width == 0 || self.height == 0 {
            return None;
        }
        // Hauteur de la page en points du repere affiche.
        let page_h = self.height as f32 / scale;
        let [x1, y1, x2, y2] = rect;
        let mut samples: Vec<[u8; 3]> = Vec::with_capacity(6);
        for y in [y2 + 2.0_f32, y1 - 2.0] {
            for t in [0.2_f32, 0.5, 0.8] {
                let x = x1 + (x2 - x1) * t;
                let px = (x * scale) as i64;
                let py = ((page_h - y) * scale) as i64;
                if px < 0 || py < 0 || px >= self.width as i64 || py >= self.height as i64 {
                    continue;
                }
                let idx = (py as usize * self.width as usize + px as usize) * 4;
                if idx + 2 < self.pixels.len() {
                    samples.push([
                        self.pixels[idx],
                        self.pixels[idx + 1],
                        self.pixels[idx + 2],
                    ]);
                }
            }
        }
        if samples.is_empty() {
            return None;
        }
        let mut out = [0u8; 3];
        for (channel, slot) in out.iter_mut().enumerate() {
            let mut values: Vec<u8> = samples.iter().map(|s| s[channel]).collect();
            values.sort_unstable();
            *slot = values[values.len() / 2];
        }
        Some(out)
    }

    /// Construit un `egui::ColorImage` à partir du bitmap.
    ///
    /// Conversion directe : pdfium produit du BGRA, egui attend du RGBA.
    pub fn to_egui_image(&self) -> egui::ColorImage {
        let expected = self.width as usize * self.height as usize * 4;
        let size = [self.width as usize, self.height as usize];
        if self.pixels.len() < expected {
            return egui::ColorImage::new(size, egui::Color32::from_rgb(240, 240, 240));
        }
        // `PdfBitmap::as_rgba_bytes` normalise DÉJÀ le tampon en RGBA (il
        // retourne le BGRx natif de pdfium dans l'ordre RGBA). On recopiait
        // R et B l'un dans l'autre : un rouge pur (#FF0000) sortait en bleu
        // (#0000FF) à l'écran, et les tampons `PageBitmap::solid` — déjà
        // RGBA — étaient eux aussi inversés.
        //
        // Un `Color32` fait exactement quatre octets : on le construit
        // directement par pixel, sans tampon RGBA intermédiaire que
        // `from_rgba_unmultiplied` reparcourrait ensuite. L'alpha reste
        // forcé à 255 : `as_rgba_bytes` conserve celui de la source (le
        // quatrième octet d'un BGRx, pas garanti opaque).
        let pixels = self.pixels[..expected]
            .chunks_exact(4)
            .map(|chunk| egui::Color32::from_rgb(chunk[0], chunk[1], chunk[2]))
            .collect();
        egui::ColorImage { size, pixels }
    }
}

/// Interface de rendu.
///
/// Pas de borne `Send + Sync` : pdfium n'est pas thread-safe, et le
/// compilateur garantit ainsi que le renderer reste sur le thread UI.
pub trait Renderer {
    /// Rend une page (1-indexée) en bitmap.
    fn render_page(
        &self,
        doc: &Document,
        page_number: u32,
        options: &RenderOptions,
    ) -> Result<PageBitmap>;

    /// Vrai si le rendu reflète le **vrai** contenu des pages.
    ///
    /// [`NullRenderer`] renvoie `false` : il fabrique un rectangle uni. Les
    /// opérations destructives (caviardage, OCR qui remplace les pages)
    /// doivent refuser un tel renderer, sinon elles écrasent le contenu du
    /// document avec du vide **en annonçant un succès**.
    fn renders_real_content(&self) -> bool {
        true
    }

    /// Extrait le texte d'une page (1-indexée), un fragment par segment.
    ///
    /// L'implémentation pdfium décode le texte réel, y compris les polices
    /// CID dont le flux ne donne que des codes : c'est la seule source
    /// correcte pour ces documents. La valeur par défaut retombe sur le
    /// décodeur interne de [`Document`], seule option de [`NullRenderer`].
    fn page_fragments(&self, doc: &Document, page_number: u32) -> Result<Vec<TextFragment>> {
        doc.extract_text_fragments(page_number)
    }
}

/// Rendu factice, utilisé si pdfium n'est pas disponible.
///
/// Produit un rectangle gris clair de la taille demandée. L'application
/// reste fonctionnelle (navigation, zoom, onglets) mais le contenu des
/// pages n'est pas affiché.
pub struct NullRenderer;

impl Renderer for NullRenderer {
    fn render_page(
        &self,
        doc: &Document,
        page_number: u32,
        options: &RenderOptions,
    ) -> Result<PageBitmap> {
        let (w_pt, h_pt) = doc.page_size(page_number);
        let w = (w_pt * options.scale).round();
        let h = (h_pt * options.scale).round();
        // Le facteur d'échelle vient de l'utilisateur : une valeur absurde ou
        // un MediaBox démesuré produirait une allocation impossible.
        const MAX_PIXELS: f32 = 20_000.0;
        if !w.is_finite() || !h.is_finite() || w < 1.0 || h < 1.0 {
            return Err(Error::InvalidInput("échelle de rendu nulle".into()));
        }
        if w > MAX_PIXELS || h > MAX_PIXELS {
            return Err(Error::InvalidInput(format!(
                "page trop grande pour le rendu : {w:.0}×{h:.0} pixels"
            )));
        }
        Ok(PageBitmap::solid(w as u32, h as u32, [250, 250, 248, 255]))
    }

    fn renders_real_content(&self) -> bool {
        false
    }
}

pub mod impl_pdfium;
mod pdfium;
pub use pdfium::PdfiumRenderer;

