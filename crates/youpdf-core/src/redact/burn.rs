//! Brûlage de pixels : effacement définitif dans les images.
//!
//! Deux emplois :
//!
//! 1. **Brûler une image embarquée** : quand une zone caviardée recouvre une
//!    image, on ne se contente pas de dessiner un rectangle par-dessus — les
//!    pixels d'origine resteraient récupérables en éditant le PDF. On décode
//!    l'image, on écrase les pixels sous la zone avec la couleur de
//!    remplissage, puis on réencode **sur place** : l'objet image garde son
//!    identifiant, donc le nom de ressource et la matrice de placement restent
//!    valables.
//! 2. **Peindre un bitmap déjà rendu** : le repli par rasterisation dessine les
//!    zones directement dans les pixels de la page rendue.
//!
//! ## Repères
//!
//! Les bitmaps manipulés ici sont en **coordonnées écran** (origine
//! haut-gauche, y vers le bas), alors que les rectangles de caviardage sont en
//! **coordonnées affichées** (origine bas-gauche, y vers le haut) et les
//! pixels d'une image embarquée sont rangés du haut vers le bas. Les conversions
//! sont le cœur de ce module : une erreur d'axe y caviarderait le mauvais coin.

use crate::document::Document;
use crate::redact::geometry::BBox;
use crate::redact::{RedactionFill, RedactionOverlay};

use youpdf_shared::Result;

/// Couleur RGB effective d'un remplissage.
pub(crate) fn fill_rgb(fill: RedactionFill) -> [u8; 3] {
    match fill {
        RedactionFill::Black => [0, 0, 0],
        RedactionFill::White => [255, 255, 255],
        RedactionFill::Custom(rgb) => rgb,
    }
}

/// Normalise un rectangle `[x1, y1, x2, y2]` : le premier coin peut être
/// n'importe lequel, la boîte doit toujours être orientée bas-gauche →
/// haut-droite.
fn normalize(rect: [f32; 4]) -> (f32, f32, f32, f32) {
    let [x1, y1, x2, y2] = rect;
    (x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2))
}

/// Remplit un rectangle dans un bitmap RGBA déjà rendu.
///
/// Le rectangle est en coordonnées **affichées** (origine bas-gauche) ; le
/// bitmap est en coordonnées écran (origine haut-gauche). `display_w` et
/// `display_h` sont la taille affichée de la page en points : le bitmap peut
/// être rendu à une autre échelle, d'où la conversion en fractions avant de
/// passer en pixels.
pub(crate) fn paint_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    display_w: f32,
    display_h: f32,
    rect: [f32; 4],
    fill: RedactionFill,
) {
    let (rx1, ry1, rx2, ry2) = normalize(rect);

    // Une page de taille nulle n'a rien à peindre ; une fraction vaut 0 et le
    // rectangle s'écrase sur le bord.
    let fx = |x: f32| -> f32 {
        if display_w > 0.0 {
            x / display_w
        } else {
            0.0
        }
    };
    let fy = |y: f32| -> f32 {
        if display_h > 0.0 {
            y / display_h
        } else {
            0.0
        }
    };

    let px1 = (fx(rx1) * width as f32).clamp(0.0, width as f32) as u32;
    let px2 = (fx(rx2) * width as f32).clamp(0.0, width as f32) as u32;
    // L'axe y est inversé : affiché bas-gauche, écran haut-gauche.
    let py1 = ((1.0 - fy(ry2)) * height as f32).clamp(0.0, height as f32) as u32;
    let py2 = ((1.0 - fy(ry1)) * height as f32).clamp(0.0, height as f32) as u32;
    let (py1, py2) = (py1.min(py2), py1.max(py2));

    let [r, g, b] = fill_rgb(fill);
    fill_rgba(pixels, width, px1, py1, px2, py2, r, g, b);
}

/// Écrase `x1..x2` × `y1..y2` (coordonnées écran) dans un bitmap RGBA.
///
/// Les bornes sont bornées deux fois (à l'appel et ici) parce qu'un rectangle
/// affiché peut dépasser la page : on écrase ce qui dépasse, jamais un pixel
/// hors du tableau.
#[allow(clippy::too_many_arguments)]
fn fill_rgba(
    pixels: &mut [u8],
    width: u32,
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    r: u8,
    g: u8,
    b: u8,
) {
    let x2 = x2.min(width);
    let y2 = y2.min((pixels.len() / 4) as u32 / width.max(1));
    let x1 = x1.min(x2);
    let y1 = y1.min(y2);
    for y in y1..y2 {
        let row = (y * width) as usize;
        for x in x1..x2 {
            let idx = (row + x as usize) * 4;
            if idx + 3 < pixels.len() {
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = 255;
            }
        }
    }
}

/// Table des glyphes 5×7, indexée par `caractère - 0x20`.
///
/// Chaque glyphe tient sur 7 octets, un par ligne, et chaque octet porte 5 bits
/// (bit 4 = colonne gauche, bit 0 = colonne droite). C'est le codage classique
/// des polices matricielles : compact à l'écriture, lecture directe au
/// tracé. Les caractères non ASCII (au-delà de 0x7E) ne sont pas représentés ;
/// ils sont remplacés par une boîte creuse.
const FONT_5X7: [[u8; 7]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
    [0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00],
    [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
    [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
    [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
    [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D],
    [0x0C, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
    [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
    [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
    [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00],
    [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
    [0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08],
    [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
    [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x00],
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
    [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
    [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x04, 0x08],
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
    [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    [0x0E, 0x11, 0x01, 0x0D, 0x15, 0x16, 0x0E],
    [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
    [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
    [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
    [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
    [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
    [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
    [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
    [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
    [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
    [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
    [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
    [0x00, 0x10, 0x08, 0x04, 0x02, 0x01, 0x00],
    [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
    [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
    [0x08, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00],
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F],
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E],
    [0x00, 0x00, 0x0E, 0x10, 0x10, 0x11, 0x0E],
    [0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F],
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E],
    [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08],
    [0x00, 0x0F, 0x11, 0x11, 0x0F, 0x01, 0x0E],
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11],
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E],
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C],
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12],
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x15, 0x15],
    [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11],
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E],
    [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10],
    [0x00, 0x00, 0x0D, 0x13, 0x0F, 0x01, 0x01],
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10],
    [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E],
    [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06],
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D],
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04],
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A],
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11],
    [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E],
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F],
    [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02],
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
    [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08],
    [0x00, 0x00, 0x08, 0x15, 0x02, 0x00, 0x00],
];

/// Glyphe de remplacement pour un caractère hors ASCII imprimable.
const GLYPH_UNKNOWN: [u8; 7] = [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F];

/// Rend un glyphe : sa matrice 5×7, ou une boîte creuse si le caractère est
/// hors de la plage ASCII imprimable.
fn glyph_for(ch: char) -> [u8; 7] {
    if ch.is_ascii() {
        let code = ch as u8;
        if (0x20..=0x7E).contains(&code) {
            return FONT_5X7[(code - 0x20) as usize];
        }
    }
    GLYPH_UNKNOWN
}

/// Surface de dessin : un tampon d'octets, ses dimensions et le nombre
/// d'octets par pixel (4 pour du RGBA, 3 pour du RGB).
///
/// Les deux formats partagent le même code de tracé ; seuls le nombre
/// d'octets par pixel et la présence d'un canal alpha diffèrent.
struct Surface<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
    components: usize,
}

impl Surface<'_> {
    /// Écrit un pixel ; rien n'est fait s'il sort du tampon.
    fn put(&mut self, x: u32, y: u32, color: [u8; 3]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = ((y as usize) * self.width as usize + x as usize) * self.components;
        if offset + self.components > self.pixels.len() {
            return;
        }
        self.pixels[offset] = color[0];
        self.pixels[offset + 1] = color[1];
        self.pixels[offset + 2] = color[2];
        if self.components == 4 {
            self.pixels[offset + 3] = 255;
        }
    }

    /// Remplit un rectangle, bornes en pixels, semi-ouvertes.
    fn fill_rect(&mut self, x1: u32, y1: u32, x2: u32, y2: u32, color: [u8; 3]) {
        let x2 = x2.min(self.width);
        let y2 = y2.min(self.height);
        let x1 = x1.min(x2);
        let y1 = y1.min(y2);
        // Les bornes sont déjà dans le tampon : on écrit directement, sans
        // repasser par `put` à chaque pixel — une zone de caviardage peut
        // couvrir des centaines de milliers de pixels.
        let row_bytes = self.width as usize * self.components;
        for y in y1..y2 {
            let start = y as usize * row_bytes + x1 as usize * self.components;
            let end = y as usize * row_bytes + x2 as usize * self.components;
            if end > self.pixels.len() {
                break;
            }
            for chunk in self.pixels[start..end].chunks_exact_mut(self.components) {
                chunk[0] = color[0];
                chunk[1] = color[1];
                chunk[2] = color[2];
                if self.components == 4 {
                    chunk[3] = 255;
                }
            }
        }
    }

    /// Trace une ligne de texte.
    ///
    /// `top_edge` est la coordonnée verticale du **bord supérieur** du bloc de
    /// texte, dans le repère de la surface ; le bloc s'étend vers le bas sur
    /// `scale` pixels. `scale` est la hauteur du bloc en pixels.
    fn draw_glyphs(&mut self, x: f32, top_edge: f32, text: &str, scale: f32, color: [u8; 3]) {
        if text.is_empty() || !scale.is_finite() || scale <= 0.0 {
            return;
        }
        // Un point de la matrice 5×7 occupe un carré de `scale/7` de côté :
        // sept de haut, cinq de large, et le glyphe garde ainsi ses
        // proportions au lieu d'être étiré.
        let cell = scale / 7.0;
        for (index, ch) in text.chars().enumerate() {
            let glyph = glyph_for(ch);
            for (row, &bits) in glyph.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                let ry0 = top_edge + row as f32 * cell;
                let ry1 = ry0 + cell;
                let py0 = clamp_px(ry0.floor(), self.height);
                let py1 = clamp_px(ry1.ceil(), self.height);
                if py0 >= py1 {
                    continue;
                }
                for col in 0..5usize {
                    // Le bit 4 est la colonne de gauche, le bit 0 celle de droite.
                    if bits & (1 << (4 - col)) == 0 {
                        continue;
                    }
                    // Six colonnes par caractère : cinq de glyphe, une d'espace.
                    let cx0 = x + (index as f32 * 6.0 + col as f32) * cell;
                    let cx1 = cx0 + cell;
                    let px0 = clamp_px(cx0.floor(), self.width);
                    let px1 = clamp_px(cx1.ceil(), self.width);
                    if px0 >= px1 {
                        continue;
                    }
                    for y in py0..py1 {
                        for px in px0..px1 {
                            self.put(px, y, color);
                        }
                    }
                }
            }
        }
    }
}

/// Borne une coordonnée de pixel dans `[0, max]`.
///
/// Les NaN tombent à zéro : une coordonnée fausse doit effacer le texte,
/// pas le faire sortir du tampon.
fn clamp_px(value: f32, max: u32) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    if value <= 0.0 {
        0
    } else if value >= max as f32 {
        max
    } else {
        value as u32
    }
}

/// Trace une ligne de texte ASCII dans un bitmap RGBA8.
///
/// `x` et `y` désignent le coin **bas-gauche** du texte, en pixels, dans un
/// repère dont l'origine est le coin bas-gauche du bitmap (l'axe y monte
/// vers le haut) ; le bitmap est pourtant rangé en lignes d'écran, origine
/// en haut. `scale` est la hauteur d'une ligne, en pixels.
///
/// Le texte est agrandi par blocs de `scale/7` pixels : peindre un seul
/// pixel par point de la matrice rendrait une mention de douze points
/// illisible, alors qu'elle doit se lire sur la capture d'écran du projet.
#[allow(clippy::too_many_arguments)]
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
    if width == 0 || height == 0 {
        return;
    }
    let mut surface = Surface {
        pixels,
        width,
        height,
        components: 4,
    };
    // Le bord bas du texte est à `y` pixels au-dessus du bas du bitmap ; on
    // repasse donc en lignes d'écran, dont l'origine est en haut.
    let top_edge = height as f32 - y - scale;
    surface.draw_glyphs(x, top_edge, text, scale, color);
}

/// Écrit un texte ASCII dans un buffer d'image RGB8 (trois octets par pixel).
///
/// Le repère est celui de l'image : ligne 0 en haut, colonne 0 à gauche.
/// `x` est la colonne du bord gauche du texte et `y` la **ligne du bas du
/// glyphe** — la dernière ligne de la cellule 5×7 ; le bloc de texte s'étend
/// donc vers le haut, de `scale` pixels. Ce repère diffère de celui de
/// [`draw_text`], qui travaille sur un bitmap écran : d'où une fonction
/// distincte plutôt qu'un paramètre de plus.
#[allow(clippy::too_many_arguments)]
fn draw_text_rgb(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    text: &str,
    scale: f32,
    color: [u8; 3],
) {
    if width == 0 || height == 0 {
        return;
    }
    let mut surface = Surface {
        pixels,
        width,
        height,
        components: 3,
    };
    // `y` désigne la dernière ligne occupée : le bord supérieur du bloc est
    // `scale` pixels plus haut.
    let top_edge = y + 1.0 - scale;
    surface.draw_glyphs(x, top_edge, text, scale, color);
}

/// Brûle les zones caviardées dans une image embarquée.
///
/// `page_bbox` est la boîte de l'image sur la page, en repère page ;
/// `regions` sont les zones à effacer, également en repère page.
///
/// L'image est décodée, les pixels sous les zones sont écrasés avec la
/// couleur de remplissage, puis l'image est réencodée **sur place** : l'objet
/// garde son identifiant, donc l'opérateur `Do` et le nom de ressource
/// restent valides.
///
/// Renvoie `true` si des pixels ont effectivement été remplacés.
pub(crate) fn burn_image(
    doc: &mut Document,
    page_number: usize,
    resource_name: &str,
    page_bbox: &BBox,
    regions: &[BBox],
    fill: RedactionFill,
    overlay: Option<&RedactionOverlay>,
) -> Result<bool> {
    // Le nom de ressource peut être donné avec ou sans la barre oblique
    // initiale (`/Im0` ou `Im0`) : on compare sur la forme « nue ».
    let wanted = resource_name.trim_start_matches('/');
    let images = doc.extract_images(page_number as u32)?;
    let Some(found) = images
        .iter()
        .find(|image| image.resource_name.trim_start_matches('/') == wanted)
    else {
        // L'image n'est pas celle qu'on attendait (ou n'existe plus) : rien
        // à brûler, mais ce n'est pas une erreur.
        return Ok(false);
    };
    let info = found.clone();

    // La boîte de placement doit avoir une surface : sans elle, la
    // conversion page → pixel n'a pas de sens. Un NaN est refusé aussi :
    // il rendrait tous les pixels calculés invalides.
    let span_x = page_bbox.x2 - page_bbox.x1;
    let span_y = page_bbox.y2 - page_bbox.y1;
    if !span_x.is_finite() || !span_y.is_finite() || span_x <= 0.0 || span_y <= 0.0 {
        return Ok(false);
    }

    let (mut pixels, width, height) = doc.image_pixels(&info)?;
    if width == 0 || height == 0 {
        return Ok(false);
    }

    // L'image est posée dans le carré unité `[0,1]²` transformé par
    // `page_bbox` ; on ramène donc une coordonnée page à la fraction
    // correspondante, puis au pixel.
    //
    // **Approximation** : `page_bbox` est une boîte *alignée sur les axes*.
    // Si l'image a été posée en rotation, cette boîte ne décrit plus le
    // placement réel et le rectangle brûlé peut être décalé. L'appelant ne
    // enregistre aujourd'hui que des placements non tournés ; le jour où il
    // en enregistrera d'autres, il faudra passer la matrice de placement
    // plutôt que sa boîte englobante.
    let to_px = |page_x: f32| -> f32 { (page_x - page_bbox.x1) / span_x * width as f32 };
    // L'axe y du PDF monte, la ligne 0 d'une image est en haut : d'où
    // l'inversion.
    let to_py = |page_y: f32| -> f32 { (1.0 - (page_y - page_bbox.y1) / span_y) * height as f32 };

    let color = fill_rgb(fill);
    let mut painted = false;

    for region in regions {
        // La région est normalisée avant tout test : la forme du rectangle
        // donnée par l'appelant n'est pas garantie.
        let rx1 = region.x1.min(region.x2);
        let rx2 = region.x1.max(region.x2);
        let ry1 = region.y1.min(region.y2);
        let ry2 = region.y1.max(region.y2);

        // Zone sans recouvrement avec l'image : rien à faire.
        if rx2 < page_bbox.x1
            || rx1 > page_bbox.x2
            || ry2 < page_bbox.y1
            || ry1 > page_bbox.y2
        {
            continue;
        }

        // Bornes en pixels. `to_py(ry2)` est la ligne du haut de la zone
        // (l'axe y du PDF monte) et `to_py(ry1)` celle du bas.
        let px1 = clamp_px(to_px(rx1).floor(), width);
        let px2 = clamp_px(to_px(rx2).ceil(), width);
        let row_top = clamp_px(to_py(ry2).floor(), height);
        let row_bottom = clamp_px(to_py(ry1).ceil(), height);
        if px1 >= px2 || row_top >= row_bottom {
            continue;
        }

        {
            let mut surface = Surface {
                pixels: &mut pixels,
                width,
                height,
                components: 3,
            };
            surface.fill_rect(px1, row_top, px2, row_bottom, color);
        }
        painted = true;

        // Mention superposée : elle est centrée dans la zone, comme le
        // rectangle de remplacement dessiné par le moteur de flux.
        if let Some(overlay) = overlay {
            let scale_px = overlay.size * height as f32 / span_y;
            if scale_px > 0.0 {
                // Six colonnes par caractère, de la hauteur `scale_px/7`.
                let text_w = overlay.text.chars().count() as f32 * 6.0 * scale_px / 7.0;
                let centre_x = (to_px(rx1) + to_px(rx2)) / 2.0;
                let centre_y = (to_py(ry2) + to_py(ry1)) / 2.0;
                draw_text_rgb(
                    &mut pixels,
                    width,
                    height,
                    centre_x - text_w / 2.0,
                    centre_y + scale_px / 2.0,
                    &overlay.text,
                    scale_px,
                    overlay.color,
                );
            }
        }
    }

    if !painted {
        return Ok(false);
    }

    // Réécriture sur place : même identifiant d'objet, donc le `Do` et le
    // nom de ressource de la page restent valides.
    doc.replace_image_pixels(&info, &pixels, width, height)?;
    Ok(true)
}
