//! Édition de contenu : extraction de texte positionné et d'images.
//!
//! ## Pourquoi ce module
//!
//! Modifier le texte dans un PDF est **fondamentalement difficile** :
//! le contenu est une séquence d'instructions graphiques (`BT`, `Tj`,
//! `Td`…), pas une structure éditable. Pour changer un mot, il faut
//! soit régénérer le flux de contenu, soit recouvrir le texte original
//! par un rectangle de couleur et y placer du nouveau texte.
//!
//! YouPDF adopte une approche pragmatique et **transparente** :
//!
//! 1. **Extraction** : on identifie chaque fragment de texte avec sa
//!    position, sa police et sa taille. Cela alimente l'interface
//!    d'édition (cliquer sur un mot, le modifier).
//!
//! 2. **Remplacement** : on applique un « patch » qui consiste à
//!    dessiner un rectangle blanc sur l'ancien texte, puis à écrire
//!    le nouveau au même endroit, avec une police standard.
//!
//! Cette approche est **visuellement fiable** pour les corrections
//! simples (coquilles, remplacements ponctuels). Elle n'est **pas**
//! un vrai retraitement typographique, et l'utilisateur doit le savoir.
//!
//! ## Ce qui n'est pas fait
//!
//! - Reprogrammation complète des flux de contenu (trop fragile).
//! - Préservation exacte des polices non standard.
//! - Réécriture des paragraphes avec retour à la ligne.


pub mod crop;
pub mod extract;
pub mod image;
pub mod matrix;
pub mod replace;
pub mod text_helpers;
pub mod util;

pub use crop::{crop_fractions_in_image, response_changed};
pub use image::{ImageCrop, ImageTransform};
pub use util::{encode_pdf_literal, load_image_rgb};
pub(crate) use util::winansi_byte_pub;

/// Un fragment de texte avec sa position et son style.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// Numéro de page (1-indexé).
    pub page: usize,
    /// Boîte de chaque caractère, quand le moteur la connaît. Vide pour le
    /// décodeur interne : l'interface retombe alors sur une répartition
    /// proportionnelle dans `rect`.
    pub chars: Vec<TextChar>,
    /// Texte.
    pub text: String,
    /// Rectangle englobant en coordonnées PDF `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// Nom de la police (informative).
    pub font: Option<String>,
    /// Taille de police en points.
    pub font_size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
}

/// Un caractère et sa boîte, en repère page.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChar {
    /// Le caractère.
    pub ch: char,
    /// Rectangle `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
}

/// Police standard-14 utilisée par un patch de texte.
///
/// Le patch écrit par-dessus le contenu d'origine avec une police dédiée : il
/// ne peut donc pas reprendre la police réelle du document, mais peut au moins
/// en choisir une parmi celles que tout lecteur connaît, sans incorporer de
/// fichier de police.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PatchFont {
    /// Sans empattement (Helvetica).
    #[default]
    Helvetica,
    /// À empattements (Times).
    Times,
    /// À chasse fixe (Courier).
    Courier,
}

impl PatchFont {
    /// Les trois familles proposées.
    pub const ALL: [PatchFont; 3] = [PatchFont::Helvetica, PatchFont::Times, PatchFont::Courier];

    /// Nom du fichier de base PDF pour cette famille, dans la variante
    /// demandée (gras / italique).
    pub fn base_font(self, bold: bool, italic: bool) -> &'static str {
        match (self, bold, italic) {
            (PatchFont::Helvetica, false, false) => "Helvetica",
            (PatchFont::Helvetica, true, false) => "Helvetica-Bold",
            (PatchFont::Helvetica, false, true) => "Helvetica-Oblique",
            (PatchFont::Helvetica, true, true) => "Helvetica-BoldOblique",
            (PatchFont::Times, false, false) => "Times-Roman",
            (PatchFont::Times, true, false) => "Times-Bold",
            (PatchFont::Times, false, true) => "Times-Italic",
            (PatchFont::Times, true, true) => "Times-BoldItalic",
            (PatchFont::Courier, false, false) => "Courier",
            (PatchFont::Courier, true, false) => "Courier-Bold",
            (PatchFont::Courier, false, true) => "Courier-Oblique",
            (PatchFont::Courier, true, true) => "Courier-BoldOblique",
        }
    }

    /// Libellé affiché dans l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            PatchFont::Helvetica => "Helvetica",
            PatchFont::Times => "Times",
            PatchFont::Courier => "Courier",
        }
    }

    /// Largeur moyenne d'un caractère, en millièmes de cadratin.
    ///
    /// C'est une estimation : les tables de largeur des polices standard ne
    /// sont pas dans le fichier PDF. Elle ne sert qu'à poser un alignement
    /// centré ou à droite, jamais à décider d'un débordement.
    fn average_width(self) -> f32 {
        match self {
            PatchFont::Helvetica => 0.52,
            PatchFont::Times => 0.46,
            PatchFont::Courier => 0.6,
        }
    }
}

/// Alignement du texte patché dans la boîte du fragment d'origine.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextAlign {
    /// Collé au bord gauche de la boîte.
    #[default]
    Left,
    /// Centré dans la boîte.
    Center,
    /// Collé au bord droit de la boîte.
    Right,
    /// Étiré sur toute la largeur de la boîte : les mots sont répartis, pas
    /// les lettres. Une ligne d'un seul mot reste à gauche.
    Justify,
}

impl TextAlign {
    /// Les quatre alignements, dans l'ordre de l'interface.
    pub const ALL: [TextAlign; 4] = [
        TextAlign::Left,
        TextAlign::Center,
        TextAlign::Right,
        TextAlign::Justify,
    ];

    /// Libellé affiché dans l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Center => "Center",
            TextAlign::Right => "Right",
            TextAlign::Justify => "Justify",
        }
    }
}

/// Mise en forme d'un patch de texte.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Famille de police.
    pub font: PatchFont,
    /// Corps, en points.
    pub size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
    /// Gras.
    pub bold: bool,
    /// Italique.
    pub italic: bool,
    /// Souligné.
    pub underline: bool,
    /// Barré.
    pub strikeout: bool,
    /// Alignement dans la boîte d'origine.
    pub align: TextAlign,
    /// Interligne, en multiples du corps.
    pub line_spacing: f32,
    /// Approche (espace ajouté après chaque caractère), en points.
    pub char_spacing: f32,
}

impl TextStyle {
    /// Les corps proposés dans l'interface.
    pub const SIZES: [f32; 12] = [
        8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 24.0, 36.0, 48.0, 72.0,
    ];

    /// Interlignes proposées dans l'interface.
    pub const LINE_SPACINGS: [f32; 4] = [1.0, 1.15, 1.5, 2.0];

    /// Style d'un fragment tel qu'extrait du document : c'est ce qui est
    /// appliqué quand l'utilisateur ne touche qu'au texte.
    pub fn from_fragment(fragment: &TextFragment) -> Self {
        TextStyle {
            font: PatchFont::Helvetica,
            size: fragment.font_size,
            color: fragment.color,
            bold: false,
            italic: false,
            underline: false,
            strikeout: false,
            align: TextAlign::Left,
            line_spacing: 1.0,
            char_spacing: 0.0,
        }
    }
}

/// Largeur estimée d'une ligne, en points.
///
/// Approximation assumée : faute des tables de largeur des polices standard,
/// on compte une largeur moyenne par caractère. Courier étant à chasse fixe,
/// son compte est exact au dixième de cadratin près.
pub fn estimate_text_width(text: &str, style: &TextStyle) -> f32 {
    let chars = text.chars().count() as f32;
    // Le corps est celui que le flux écrit réellement : sans cette borne, un
    // corps dégénéré ferait diverger l'alignement de la largeur rendue.
    let size = style.size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
    chars * size * style.font.average_width() + chars * style.char_spacing
}

/// Corps minimal accepté par un patch : en dessous, le texte est illisible et
/// les traits du souligné se confondent.
const MIN_TEXT_SIZE: f32 = 1.0;

/// Corps maximal accepté par un patch : au-delà, la ligne déborde de toute
/// page.
const MAX_TEXT_SIZE: f32 = 500.0;

/// Nom de ressource PDF de la police de patch pour un fichier de base donné.
///
/// Helvetica garde `YouPdfF1`, le nom historique : les documents déjà produits
/// et les tests qui le vérifient restent valides.
pub fn patch_font_name(base_font: &str) -> String {
    if base_font == "Helvetica" {
        "YouPdfF1".to_owned()
    } else {
        format!("YouPdf{}", base_font.replace('-', ""))
    }
}

/// Une plage de texte à l'intérieur d'un fragment.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub fragment_index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Informations sur une image embarquée dans un document.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    /// Page contenant l'image.
    pub page: usize,
    /// Nom de ressource (`/Im0`, `/Im1`…).
    pub resource_name: String,
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
    /// Nombre de composantes couleur (1, 3 ou 4).
    pub components: u8,
    /// Référence de l'objet image dans le document.
    pub object_id: (u32, u16),
}
