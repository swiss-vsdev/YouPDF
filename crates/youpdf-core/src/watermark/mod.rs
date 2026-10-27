//! Filigranes, en-têtes et pieds de page, numéros de page.
//!
//! ## Approche retenue
//!
//! Comme le remplacement de texte de [`crate::edit`], tout est écrit
//! **dans le flux de contenu** de la page : le filigrane s'imprime, se
//! convertit en image, et reste visible dans n'importe quel lecteur. Une
//! annotation ne donnerait qu'un objet décrit au `/Annots`, que certains
//! lecteurs n'affichent pas.
//!
//! Chaque page décorée reçoit un flux de contenu supplémentaire, posé à
//! la fin de `/Contents` et marqué par la clé `/YouPdfDecoration` : c'est
//! ce marquage qui permet de tout retirer ensuite avec
//! [`Document::clear_watermarks`].
//!
//! ## Ressources posées par ce module
//!
//! | Nom              | Type        | Rôle                        |
//! |------------------|-------------|-----------------------------|
//! | `/YouPdfF1`      | `/Font`     | Helvetica /WinAnsiEncoding  |
//! | `/YouPdfWMGS0`   | `/ExtGState`| opacité du texte (`/ca`)    |

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::{Document, PageView};
use crate::edit::{encode_pdf_literal, load_image_rgb};
use crate::util::flate_compress;


mod apply;
mod ops;

pub(crate) use ops::{
    background_color_ops, background_image_ops, background_image_stream, bates_ops,
    pdf_watermark_ops,
    header_footer_ops, marked_dict, marked_dict_with, to_roman, watermark_ops,
};

/// Nom de ressource de la police posée par ce module.
const WM_FONT_NAME: &str = "YouPdfF1";

/// Nom de ressource de l'état graphique (opacité) posé par ce module.
const WM_GS_NAME: &str = "YouPdfWMGS0";

/// Clé marquant un flux de contenu posé par ce module.
const WM_MARKER: &str = "YouPdfDecoration";

/// Clé marquant un flux d'arrière-plan posé par ce module.
const BG_MARKER: &str = "YouPdfBackground";

/// Clé marquant un flux de numérotation Bates posé par ce module.
const BATES_MARKER: &str = "YouPdfBates";

/// Nom de ressource du XObject image d'arrière-plan.
const BG_XOBJECT_NAME: &str = "YouPdfBG0";

/// Nom de ressource du XObject image d'un filigrane PDF.
const WM_PDF_XOBJECT_NAME: &str = "YouPdfWM0";

/// Pages auxquelles un filigrane ou un en-tête s'applique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRange {
    /// Toutes les pages.
    All,
    /// De cette page (1-indexée) jusqu'à la dernière.
    From(u32),
    /// Intervalle inclusif, bornes 1-indexées.
    Range(u32, u32),
}

impl PageRange {
    /// Vrai si la page (1-indexée) est dans la plage.
    pub fn contains(&self, page: u32) -> bool {
        match *self {
            PageRange::All => true,
            PageRange::From(start) => page >= start,
            PageRange::Range(start, end) => (start..=end).contains(&page),
        }
    }

    /// Pages concernées pour un document de `total` pages, bornes bornées.
    fn pages(&self, total: u32) -> Vec<u32> {
        if total == 0 {
            return Vec::new();
        }
        let (start, end) = match *self {
            PageRange::All => (1, total),
            PageRange::From(start) => (start.max(1), total),
            PageRange::Range(start, end) => (start.max(1), end.min(total)),
        };
        if start > end {
            return Vec::new();
        }
        (start..=end).collect()
    }
}

/// Style de numérotation d'une page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// `1`
    Numeric,
    /// `1 / 12`
    NumericTotal,
    /// `i` (chiffres romains minuscules).
    Roman,
}

impl NumberStyle {
    /// Rend le numéro `page` sur un document de `total` pages.
    pub fn render(&self, page: u32, total: u32) -> String {
        match self {
            NumberStyle::Numeric => page.to_string(),
            NumberStyle::NumericTotal => format!("{page} / {total}"),
            NumberStyle::Roman => to_roman(page),
        }
    }
}

/// Ancrage du filigrane sur la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WatermarkPosition {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    /// Centre de la page (défaut).
    #[default]
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Description d'un filigrane texte.
#[derive(Debug, Clone)]
pub struct Watermark {
    /// Texte affiché.
    pub text: String,
    /// Taille de police en points.
    pub font_size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
    /// Opacité (0.0 à 1.0).
    pub opacity: f32,
    /// Rotation en degrés (45 = diagonale classique).
    pub rotation_deg: f32,
    /// Variation aléatoire de rotation par tuile (0 = aucune variation).
    /// En degrés, ex: 15 = ±15° autour de `rotation_deg`.
    pub rotation_randomness: f32,
    /// Répète le texte sur toute la page au lieu d'un seul au centre.
    pub tiled: bool,
    /// Pages concernées.
    pub pages: PageRange,
    /// Ancrage du texte (ignoré quand le filigrane est carrelé).
    pub position: WatermarkPosition,
    /// Décalage appliqué à l'ancre, en points.
    pub offset_x: f32,
    pub offset_y: f32,
    /// Espace entre deux répétitions d'un filigrane carrelé, en points.
    pub tile_gap: f32,
    /// Poser le filigrane derrière le contenu de la page.
    pub behind: bool,
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: "CONFIDENTIEL".to_string(),
            font_size: 48.0,
            color: [128, 128, 128],
            opacity: 0.25,
            rotation_deg: 45.0,
            rotation_randomness: 0.0,
            tiled: false,
            pages: PageRange::All,
            position: WatermarkPosition::Center,
            offset_x: 0.0,
            offset_y: 0.0,
            tile_gap: 120.0,
            behind: false,
        }
    }
}

/// Jeu de textes d'une bande : gauche, centre, droite.
///
/// Un en-tête et un pied de page ont chacun le leur : ce ne sont pas les mêmes
/// textes en haut et en bas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BandSlots {
    /// Texte à gauche.
    pub left: String,
    /// Texte centré.
    pub center: String,
    /// Texte à droite.
    pub right: String,
}

impl BandSlots {
    /// Vrai si les trois emplacements sont vides (espaces compris).
    pub fn is_empty(&self) -> bool {
        self.left.trim().is_empty()
            && self.center.trim().is_empty()
            && self.right.trim().is_empty()
    }
}

/// En-tête et pied de page, avec numérotation facultative.
#[derive(Debug, Clone)]
pub struct HeaderFooter {
    /// Bande du haut.
    pub header: BandSlots,
    /// Bande du bas.
    pub footer: BandSlots,
    /// Distance au bord de la page, en points.
    pub margin: f32,
    /// Taille de police en points.
    pub font_size: f32,
    /// Style de numérotation (aucun si `None`).
    pub page_numbers: Option<NumberStyle>,
    /// Pages concernées.
    pub pages: PageRange,
    /// Première page numérotée ; les précédentes portent le bandeau seul.
    pub start_page: u32,
    /// Numérote à partir de la deuxième page.
    pub skip_first_page: bool,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        Self {
            header: BandSlots::default(),
            footer: BandSlots::default(),
            margin: 36.0,
            font_size: 10.0,
            page_numbers: None,
            pages: PageRange::All,
            start_page: 1,
            skip_first_page: false,
        }
    }
}

/// Description d'un arrière-plan de page : aplat de couleur et/ou image.
#[derive(Debug, Clone)]
pub struct Background {
    /// Couleur RGB (0–255) de l'aplat.
    pub color: [u8; 3],
    /// Opacité de l'aplat (0.0 à 1.0).
    pub opacity: f32,
    /// Image de fond (PNG ou JPEG). `None` : aplat de couleur seul.
    pub image: Option<std::path::PathBuf>,
    /// Échelle de l'image : 1.0 couvre la boîte visible.
    pub scale: f32,
    /// Pages concernées.
    pub pages: PageRange,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            color: [255, 255, 255],
            opacity: 1.0,
            image: None,
            scale: 1.0,
            pages: PageRange::All,
        }
    }
}

/// Position du numéro Bates sur la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatesPosition {
    /// Coin supérieur gauche.
    TopLeft,
    /// Bord supérieur, centré.
    TopCenter,
    /// Coin supérieur droit.
    TopRight,
    /// Coin inférieur gauche.
    BottomLeft,
    /// Bord inférieur, centré.
    BottomCenter,
    /// Coin inférieur droit.
    BottomRight,
}

/// Description d'une numérotation Bates.
#[derive(Debug, Clone)]
pub struct Bates {
    /// Préfixe du libellé (`DOC-`).
    pub prefix: String,
    /// Suffixe du libellé.
    pub suffix: String,
    /// Premier numéro de la sélection.
    pub start: u32,
    /// Incrément appliqué d'une page à l'autre.
    pub step: u32,
    /// Nombre de chiffres, complété par des zéros.
    pub digits: usize,
    /// Position sur la page.
    pub position: BatesPosition,
    /// Distance au bord de la page, en points.
    pub margin: f32,
    /// Taille de police en points.
    pub font_size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
    /// Pages concernées.
    pub pages: PageRange,
}

impl Default for Bates {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            suffix: String::new(),
            start: 1,
            step: 1,
            digits: 6,
            position: BatesPosition::BottomRight,
            margin: 36.0,
            font_size: 10.0,
            color: [0, 0, 0],
            pages: PageRange::All,
        }
    }
}

