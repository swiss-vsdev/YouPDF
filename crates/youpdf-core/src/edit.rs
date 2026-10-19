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

use lopdf::{Dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Un fragment de texte avec sa position et son style.
#[derive(Debug, Clone)]
pub struct TextFragment {
    /// Numéro de page (1-indexé).
    pub page: usize,
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

// ────────────────────────────────────────────────────────────────────────
// Extraction
// ────────────────────────────────────────────────────────────────────────

impl Document {
    /// Extrait tous les fragments de texte positionnés d'une page.
    ///
    /// ## Limites
    ///
    /// L'analyse repose sur `lopdf` pour reconstruire les positions à
    /// partir des opérateurs `Td`, `TD`, `Tm`, `T*`. Les polices avec
    /// encodage non standard (CID, Type0 complexe) peuvent produire
    /// des positions imprécises. Les fragments retournés sont donc
    /// **indicatifs** : utiles pour localiser un mot à l'écran, moins
    /// fiables pour un repositionnement au point près.
    pub fn extract_text_fragments(&self, page: u32) -> Result<Vec<TextFragment>> {
        let page_id = self
            .page_id(page)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
        let page_dict = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        // Résolution de la police par défaut (peut être None).
        let default_font = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|r| match r {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
                _ => None,
            })
            .and_then(|res| res.get(b"Font").ok().cloned())
            .and_then(|f| match f {
                Object::Dictionary(d) => Some(d),
                Object::Reference(id) => self.inner().get_dictionary(id).ok().cloned(),
                _ => None,
            });

        // On parcourt le flux de contenu, à la recherche des blocs BT…ET
        // et des opérateurs de positionnement.
        let content = self
            .inner()
            .get_page_content(page_id)
            .map_err(|e| Error::Pdf(format!("contenu page {page} : {e}")))?;

        let mut fragments = Vec::new();
        let mut current: Option<TextFragment> = None;
        let mut tx: f32 = 0.0;
        let mut ty: f32 = 0.0;
        let mut font_size: f32 = 11.0;
        let mut font_name: Option<String> = None;

        // Analyse naïve du flux texte. On n'essaie pas d'être exhaustif :
        // on couvre les cas usuels (Tj, TJ, Td, TD, Tm, Tf, T*).
        let tokens = tokenize_content(&content);

        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].as_str() {
                "BT" => {
                    tx = 0.0;
                    ty = 0.0;
                }
                "Tf" => {
                    // Séquence : /Nom taille Tf
                    if i >= 2 {
                        if let Some(name) = tokens.get(i - 2) {
                            if name.starts_with('/') {
                                font_name = Some(name.trim_start_matches('/').to_string());
                            }
                        }
                        if let Some(size) = tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()) {
                            font_size = size;
                        }
                    }
                }
                "Td" | "TD" => {
                    // dx dy Td
                    if i >= 2 {
                        if let (Some(dx), Some(dy)) = (
                            tokens.get(i - 2).and_then(|s| s.parse::<f32>().ok()),
                            tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()),
                        ) {
                            tx += dx;
                            ty += dy;
                        }
                    }
                }
                "Tm" => {
                    // a b c d e f Tm : on ne garde que e, f
                    if i >= 6 {
                        if let (Some(e), Some(f)) = (
                            tokens.get(i - 2).and_then(|s| s.parse::<f32>().ok()),
                            tokens.get(i - 1).and_then(|s| s.parse::<f32>().ok()),
                        ) {
                            tx = e;
                            ty = f;
                        }
                    }
                }
                "T*" => {
                    // Retour à la ligne : on avance de la taille de police.
                    ty -= font_size * 1.2;
                }
                "Tj" | "'" | "\"" => {
                    // Le texte est le token précédent.
                    if i >= 1 {
                        if let Some(text) = extract_string_literal(&tokens[i - 1]) {
                            push_fragment(
                                &mut fragments,
                                &mut current,
                                text,
                                page as usize,
                                tx,
                                ty,
                                font_size,
                                font_name.clone(),
                            );
                            // Avance horizontale approximative.
                            tx += font_size * 0.5 * current_text_len(&fragments) as f32;
                        }
                    }
                }
                "ET" => {
                    if let Some(f) = current.take() {
                        fragments.push(f);
                    }
                }
                _ => {}
            }
            i += 1;
        }

        // Polices non résolues : on ignore.
        let _ = default_font;

        Ok(fragments)
    }

    /// Liste les images embarquées dans une page.
    pub fn extract_images(&self, page: u32) -> Result<Vec<ImageInfo>> {
        let page_id = self
            .page_id(page)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
        let page_dict = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        let resources = match page_dict.get(b"Resources") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))?
                .clone(),
            _ => return Ok(Vec::new()),
        };

        let xobjects = match resources.get(b"XObject") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("XObject : {e}")))?
                .clone(),
            _ => return Ok(Vec::new()),
        };

        let mut out = Vec::new();
        for (name, value) in xobjects.iter() {
            let Object::Reference(obj_id) = value else {
                continue;
            };
            let Ok(obj) = self.inner().get_object(*obj_id) else {
                continue;
            };
            let Ok(stream) = obj.as_stream() else {
                continue;
            };
            let dict = &stream.dict;

            // Sous-type /Image uniquement.
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype != Some(b"Image") {
                continue;
            }

            // Une dimension absente, nulle ou négative est rejetée : `as u32`
            // transformerait −1 en 4 milliards, et une allocation suivrait.
            let dimension = |key: &[u8]| -> Option<u32> {
                match dict.get(key).ok().and_then(|o| o.as_i64().ok()) {
                    Some(value) if (1..=100_000).contains(&value) => Some(value as u32),
                    _ => None,
                }
            };
            let (Some(width), Some(height)) = (dimension(b"Width"), dimension(b"Height")) else {
                continue;
            };

            let components = match dict.get(b"ColorSpace").ok().and_then(|o| o.as_name().ok()) {
                Some(b"DeviceGray") => 1,
                Some(b"DeviceRGB") => 3,
                Some(b"DeviceCMYK") => 4,
                _ => 3, // inconnu, on suppose RGB
            };

            out.push(ImageInfo {
                page: page as usize,
                resource_name: String::from_utf8_lossy(name).to_string(),
                width,
                height,
                components,
                object_id: *obj_id,
            });
        }

        Ok(out)
    }

    /// Extrait les octets bruts d'une image embarquée.
    pub fn image_bytes(&self, info: &ImageInfo) -> Result<Vec<u8>> {
        let obj = self
            .inner()
            .get_object(info.object_id)
            .map_err(|e| Error::Pdf(format!("objet image : {e}")))?;
        let stream = obj
            .as_stream()
            .map_err(|e| Error::Pdf(format!("flux image : {e}")))?;
        Ok(stream.content.clone())
    }

    /// Extrait une image embarquée et la renvoie encodée en PNG.
    ///
    /// Le contenu du flux n'est pas toujours un fichier image : `DCTDecode`
    /// est du JPEG, mais `FlateDecode` cache des échantillons bruts. On passe
    /// donc par le décodeur commun, puis on ré-encode en PNG.
    pub fn extract_image_png(&self, info: &ImageInfo) -> Result<Vec<u8>> {
        let obj = self
            .inner()
            .get_object(info.object_id)
            .map_err(|e| Error::Pdf(format!("objet image : {e}")))?;
        let stream = obj
            .as_stream()
            .map_err(|e| Error::Pdf(format!("flux image : {e}")))?;
        let image = crate::compress::decode_stream_content(stream).ok_or_else(|| {
            Error::Pdf(format!(
                "image {} ({}) non décodable : format non pris en charge",
                info.resource_name, info.object_id.0
            ))
        })?;
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| Error::Pdf(format!("encodage PNG : {e}")))?;
        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────────────
// Édition d'images posées sur une page
// ────────────────────────────────────────────────────────────────────────

/// Transformation géométrique appliquée aux pixels d'une image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageTransform {
    /// Quart de tour dans le sens horaire.
    RotateCw,
    /// Quart de tour dans le sens antihoraire.
    RotateCcw,
    /// Miroir horizontal.
    FlipHorizontal,
    /// Miroir vertical.
    FlipVertical,
}

impl ImageTransform {
    /// Les quatre transformations, dans l'ordre de l'interface.
    pub const ALL: [ImageTransform; 4] = [
        ImageTransform::RotateCw,
        ImageTransform::RotateCcw,
        ImageTransform::FlipHorizontal,
        ImageTransform::FlipVertical,
    ];

    /// Libellé affiché dans l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            ImageTransform::RotateCw => "Rotate 90° CW",
            ImageTransform::RotateCcw => "Rotate 90° CCW",
            ImageTransform::FlipHorizontal => "Flip Horizontal",
            ImageTransform::FlipVertical => "Flip Vertical",
        }
    }

    /// Vrai pour les rotations, qui échangent largeur et hauteur.
    pub fn swaps_axes(self) -> bool {
        matches!(self, ImageTransform::RotateCw | ImageTransform::RotateCcw)
    }
}

/// Fractions retirées de chaque bord d'une image, entre 0 et 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageCrop {
    /// Bord gauche.
    pub left: f32,
    /// Bord haut.
    pub top: f32,
    /// Bord droit.
    pub right: f32,
    /// Bord bas.
    pub bottom: f32,
}

impl ImageCrop {
    /// Ramène les fractions dans `[0, 1)` sans jamais vider l'image : les
    /// quatre bords sont réduits proportionnellement si leur somme atteint
    /// l'image entière.
    pub fn sanitised(mut self) -> Self {
        let clamp = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 0.98)
            } else {
                0.0
            }
        };
        self.left = clamp(self.left);
        self.right = clamp(self.right);
        self.top = clamp(self.top);
        self.bottom = clamp(self.bottom);
        let width = self.left + self.right;
        if width >= 1.0 {
            self.left /= width;
            self.right /= width;
        }
        let height = self.top + self.bottom;
        if height >= 1.0 {
            self.top /= height;
            self.bottom /= height;
        }
        self
    }

    /// Vrai quand aucun bord n'est rogné.
    pub fn is_empty(&self) -> bool {
        self.left <= f32::EPSILON
            && self.top <= f32::EPSILON
            && self.right <= f32::EPSILON
            && self.bottom <= f32::EPSILON
    }
}

impl Document {
    /// Pixels RGB 8 bits d'une image embarquée, et ses dimensions.
    ///
    /// Passe par le décodeur commun : le flux peut être du JPEG (`DCTDecode`),
    /// du PNG ou des échantillons bruts en zlib.
    pub fn image_pixels(&self, info: &ImageInfo) -> Result<(Vec<u8>, u32, u32)> {
        let png = self.extract_image_png(info)?;
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| Error::Pdf(format!("décodage image : {e}")))?;
        let rgb = decoded.to_rgb8();
        let (width, height) = rgb.dimensions();
        Ok((rgb.into_raw(), width, height))
    }

    /// Remplace les pixels d'une image en place.
    ///
    /// L'objet image garde son identifiant : le nom de ressource et la
    /// matrice posés dans le flux de la page restent valables, donc l'image
    /// ne bouge pas. Si les proportions changent, elle est étirée dans la
    /// même boîte, comme le fait un remplacement d'image dans un traitement
    /// de texte.
    pub fn replace_image_pixels(
        &mut self,
        info: &ImageInfo,
        pixels_rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        let mut stream = Self::build_image_stream(pixels_rgb, width, height)?;
        // Le masque de transparence et la table de décodage appartiennent à
        // l'image : les perdre rendrait un logo transparent opaque. `Decode` et
        // une `Mask` par plage de couleurs dépendent en revanche du nombre de
        // composantes : recopiés d'une image CMJN sur notre RGB, ils
        // décaleraient les couleurs.
        if let Ok(Object::Stream(previous)) = self.inner().get_object(info.object_id) {
            if let Ok(value) = previous.dict.get(b"SMask") {
                stream.dict.set("SMask", value.clone());
            }
            let was_rgb = previous
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|space| space.as_name().ok())
                .is_some_and(|name| name == b"DeviceRGB");
            if was_rgb {
                for key in [b"Mask".as_slice(), b"Decode"] {
                    if let Ok(value) = previous.dict.get(key) {
                        stream.dict.set(key, value.clone());
                    }
                }
            }
        }
        self.inner_mut()
            .set_object(info.object_id, Object::Stream(stream));
        Ok(())
    }

    /// Tourne ou retourne une image en place.
    ///
    /// Seule la matrice qui dessine l'image change, autour du centre de sa
    /// boîte : les pixels ne sont jamais réencodés, donc l'opération est sans
    /// perte et instantanée. Une rotation échange les côtés de la boîte si
    /// l'image n'est pas carrée, comme dans les autres lecteurs.
    pub fn transform_image(&mut self, info: &ImageInfo, transform: ImageTransform) -> Result<()> {
        let found = self.transform_image_matrix(info.page, &info.resource_name, transform)?;
        if !found {
            return Err(Error::NotFound(format!(
                "matrice de l'image {}",
                info.resource_name
            )));
        }
        Ok(())
    }

    /// Règle l'opacité d'une image, de 0 (invisible) à 1 (opaque).
    ///
    /// L'opacité passe par un `/ExtGState` : elle est donc réellement
    /// transparente sur la page, sans retouche des pixels.
    pub fn set_image_opacity(&mut self, info: &ImageInfo, alpha: f32) -> Result<()> {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        // Un état graphique par image : deux images de la même page peuvent
        // avoir deux opacités différentes.
        let name = format!("YouPdfImAlpha{}", info.resource_name.replace('/', ""));
        use lopdf::dictionary;
        let mut resources = self.page_resources(page_id)?;
        let mut states = self
            .resolve_dict(resources.get(b"ExtGState").ok())
            .unwrap_or_default();
        // Un seul objet par image : régler l'opacité au curseur ne doit pas
        // semer un dictionnaire orphelin à chaque pas.
        let existing = states
            .get(name.as_bytes())
            .ok()
            .and_then(|object| object.as_reference().ok());
        let state = dictionary! {
            "Type" => Object::Name(b"ExtGState".to_vec()),
            "ca" => Object::Real(alpha),
            "CA" => Object::Real(alpha),
        };
        let state_id = match existing {
            Some(id) => {
                self.inner_mut().set_object(id, Object::Dictionary(state));
                id
            }
            None => self.inner_mut().add_object(Object::Dictionary(state)),
        };
        states.set(name.as_str(), Object::Reference(state_id));
        resources.set("ExtGState", Object::Dictionary(states));
        self.set_page_resources(page_id, resources)?;

        let resource_name = info.resource_name.clone();
        self.edit_image_draw(page_id, &resource_name, |operations, index| {
            // Un seul `/gs` par image : régler l'opacité au curseur ne doit pas
            // empiler un opérateur à chaque pas. On retire ceux déjà posés
            // juste avant le dessin (`cm` et `gs` forment le préambule).
            let mut stale = Vec::new();
            for candidate in (0..index).rev() {
                let operation = &operations[candidate];
                match operation.operator.as_str() {
                    "gs" => {
                        // Seulement nos états : un `/gs` posé par la page pour
                        // elle-même ne nous appartient pas.
                        let ours = operation
                            .operands
                            .first()
                            .and_then(|object| object.as_name().ok())
                            == Some(name.as_bytes());
                        if ours {
                            stale.push(candidate);
                        }
                    }
                    "cm" => continue,
                    _ => break,
                }
            }
            // Les indices sont décroissants : retirer du plus grand au plus
            // petit est le seul ordre qui ne décale pas les suivants.
            for position in &stale {
                operations.remove(*position);
            }
            // Le `/gs` doit précéder le `Do` : `cm` puis `gs` puis `Do`.
            operations.insert(
                index - stale.len(),
                lopdf::content::Operation::new(
                    "gs",
                    vec![Object::Name(name.as_bytes().to_vec())],
                ),
            );
        })?;
        Ok(())
    }

    /// Rogne une image : les pixels sont découpés et la boîte de dessin
    /// rétrécit d'autant, du côté rogné.
    ///
    /// C'est un vrai rognage : l'image ne bouge pas sur la page, elle perd
    /// seulement sa bordure. L'appelant peut rogner plusieurs fois de suite.
    pub fn crop_image(&mut self, info: &ImageInfo, crop: ImageCrop) -> Result<()> {
        let crop = crop.sanitised();
        if crop.is_empty() {
            return Ok(());
        }
        let (pixels, width, height) = self.image_pixels(info)?;
        // Les bords sont arrondis au pixel : la matrice reprend ces fractions
        // effectives, sinon la boîte et l'image découpée divergent d'un pixel.
        let bounds = crop_bounds(width, height, crop);
        if bounds.2 <= bounds.0 || bounds.3 <= bounds.1 {
            return Ok(());
        }
        let effective = ImageCrop {
            left: bounds.0 as f32 / width as f32,
            right: 1.0 - bounds.2 as f32 / width as f32,
            top: bounds.1 as f32 / height as f32,
            bottom: 1.0 - bounds.3 as f32 / height as f32,
        };
        let found = self.crop_image_matrix(info.page, &info.resource_name, effective)?;
        if !found {
            return Err(Error::NotFound(format!(
                "matrice de l'image {}",
                info.resource_name
            )));
        }
        let (pixels, width, height) = crop_pixels(&pixels, width, height, bounds);
        self.replace_image_pixels(info, &pixels, width, height)
    }

    /// Retire une image de la page : le `Do` disparaît du flux et la
    /// ressource est déliée. L'objet image reste dans le fichier jusqu'au
    /// prochain [`Document::purge_unreachable`].
    pub fn delete_image(&mut self, info: &ImageInfo) -> Result<()> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let resource_name = info.resource_name.clone();
        let found = self.edit_image_draw(page_id, &resource_name, |operations, index| {
            // Le préambule de l'image (`q` … `cm`) ne sert qu'à elle : il part
            // avec le dessin, et le `Q` qui le ferme aussi, sinon le flux
            // laisserait des opérateurs déséquilibrés.
            let mut remove_from = index;
            let mut balanced = false;
            for candidate in (0..index).rev() {
                match operations[candidate].operator.as_str() {
                    "cm" | "gs" => remove_from = candidate,
                    "q" => {
                        remove_from = candidate;
                        balanced = true;
                        break;
                    }
                    _ => break,
                }
            }
            let mut remove_to = index;
            // Le `Q` ne part qu'avec le `q` de l'image : sinon il fermerait le
            // groupe graphique de la page et la suite du flux serait peinte
            // avec le mauvais état.
            if balanced
                && operations
                    .get(index + 1)
                    .is_some_and(|operation| operation.operator == "Q")
            {
                remove_to = index + 1;
            }
            operations.drain(remove_from..=remove_to);
        })?;
        if !found {
            return Err(Error::NotFound(format!(
                "dessin de l'image {resource_name}"
            )));
        }

        let mut resources = self.page_resources(page_id)?;
        if let Some(mut xobjects) = self.resolve_dict(resources.get(b"XObject").ok()) {
            xobjects.remove(resource_name.as_bytes());
            resources.set("XObject", Object::Dictionary(xobjects));
        }
        self.set_page_resources(page_id, resources)?;
        Ok(())
    }

    /// Ressources d'une page, héritées de ses parents s'il n'y en a pas,
    /// références résolues.
    ///
    /// Une page sans `/Resources` emprunte ceux de l'arbre : la copie rendue
    /// doit donc remonter la chaîne, sinon l'écriture d'un état graphique
    /// ferait disparaître les polices du document.
    pub(crate) fn page_resources(&self, page_id: (u32, u16)) -> Result<lopdf::Dictionary> {
        let mut current = page_id;
        // Garde-fou : un arbre corrompu ne doit pas boucler.
        for _ in 0..64 {
            let page = self
                .inner()
                .get_dictionary(current)
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            match page.get(b"Resources") {
                Ok(Object::Dictionary(dict)) => return Ok(dict.clone()),
                Ok(Object::Reference(id)) => {
                    return self
                        .inner()
                        .get_dictionary(*id)
                        .cloned()
                        .map_err(|e| Error::Pdf(format!("ressources : {e}")));
                }
                _ => {}
            }
            match page.get(b"Parent").ok().and_then(|parent| parent.as_reference().ok()) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        Err(Error::Pdf(
            "arbre de pages cyclique : ressources introuvables".to_owned(),
        ))
    }

    /// Dictionnaire d'une entrée de ressources, en résolvant une éventuelle
    /// référence indirecte.
    pub(crate) fn resolve_dict(&self, value: Option<&Object>) -> Option<lopdf::Dictionary> {
        match value? {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Écrit les ressources d'une page.
    fn set_page_resources(
        &mut self,
        page_id: (u32, u16),
        resources: lopdf::Dictionary,
    ) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", Object::Dictionary(resources));
        Ok(())
    }

    /// Boîte occupée par une image sur la page, en coordonnées page.
    ///
    /// La boîte rendue est le rectangle englobant du carré unité transformé
    /// par la matrice qui précède le `Do` : une image tournée donne donc une
    /// boîte droite.
    pub fn image_rect(&self, info: &ImageInfo) -> Result<Option<[f32; 4]>> {
        let Some([a, b, c, d, e, f]) = self.image_matrix(info)? else {
            return Ok(None);
        };
        let corners = [(e, f), (e + a, f + b), (e + c, f + d), (e + a + c, f + b + d)];
        let x1 = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let y1 = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let x2 = corners.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let y2 = corners.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        Ok(Some([x1, y1, x2, y2]))
    }

    /// Matrice `cm` qui dessine une image sur sa page.
    ///
    /// `(a, b)` va de la gauche vers la droite de l'image, `(c, d)` du bas vers
    /// le haut, `(e, f)` est son coin bas-gauche. `None` quand l'image est
    /// dessinée sans matrice propre (héritée d'un `q` extérieur), cas que ce
    /// module ne sait pas situer.
    pub fn image_matrix(&self, info: &ImageInfo) -> Result<Option<[f32; 6]>> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = info.resource_name.trim_start_matches('/').as_bytes().to_vec();
        for content_id in self.page_content_ids(page_id)? {
            let Ok(Object::Stream(stream)) = self.inner().get_object(content_id) else {
                continue;
            };
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(content) = lopdf::content::Content::decode(&bytes) else {
                continue;
            };
            for (index, operation) in content.operations.iter().enumerate() {
                if operation.operator != "Do" {
                    continue;
                }
                if operation
                    .operands
                    .first()
                    .and_then(|object| object.as_name().ok())
                    != Some(name.as_slice())
                {
                    continue;
                }
                let Some(position) = preceding_cm(&content.operations, index) else {
                    continue;
                };
                if let Some(matrix) = matrix_of(&content.operations[position].operands) {
                    return Ok(Some(matrix));
                }
            }
        }
        Ok(None)
    }

    /// Construit le flux d'une image RGB 8 bits non compressée.
    fn build_image_stream(pixels_rgb: &[u8], width: u32, height: u32) -> Result<Stream> {
        let expected = width as usize * height as usize * 3;
        if pixels_rgb.len() < expected {
            return Err(Error::InvalidInput(format!(
                "bitmap trop court : {} octets, attendu {expected}",
                pixels_rgb.len()
            )));
        }
        use lopdf::dictionary;
        let dict = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        };
        Ok(Stream::new(dict, pixels_rgb[..expected].to_vec()))
    }

    /// Identifiants des flux de contenu d'une page.
    fn page_content_ids(&self, page_id: (u32, u16)) -> Result<Vec<(u32, u16)>> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let mut ids = Vec::new();
        match page.get(b"Contents").map_err(|e| Error::Pdf(format!("contenu : {e}")))? {
            Object::Reference(id) => ids.push(*id),
            Object::Array(items) => {
                for item in items {
                    if let Ok(id) = item.as_reference() {
                        ids.push(id);
                    }
                }
            }
            _ => {}
        }
        Ok(ids)
    }

    /// Applique `edit` aux opérateurs qui dessinent l'image `resource_name`.
    ///
    /// Le flux de contenu est décodé, les `Do` de l'image sont passés à
    /// `edit` dans l'ordre inverse (pour que les indices restent valables),
    /// puis le flux est réécrit en clair. Rend vrai si un dessin a été
    /// trouvé.
    fn edit_image_draw(
        &mut self,
        page_id: (u32, u16),
        resource_name: &str,
        mut edit: impl FnMut(&mut Vec<lopdf::content::Operation>, usize),
    ) -> Result<bool> {
        let targets = self.page_content_ids(page_id)?;
        let name = resource_name.trim_start_matches('/').as_bytes().to_vec();
        let mut found = false;
        for content_id in targets {
            let Ok(Object::Stream(stream)) = self.inner().get_object(content_id) else {
                continue;
            };
            // `decompressed_content` rend un flux vide quand il n'y a pas de
            // filtre : `get_plain_content` gère les deux cas.
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(mut content) = lopdf::content::Content::decode(&bytes) else {
                continue;
            };
            let mut indices: Vec<usize> = content
                .operations
                .iter()
                .enumerate()
                .filter(|(_, operation)| {
                    operation.operator == "Do"
                        && operation
                            .operands
                            .first()
                            .and_then(|object| object.as_name().ok())
                            == Some(name.as_slice())
                })
                .map(|(index, _)| index)
                .collect();
            if indices.is_empty() {
                continue;
            }
            // De la fin vers le début : insérer ou retirer un opérateur ne
            // décale pas les indices encore à traiter.
            indices.reverse();
            for index in indices {
                edit(&mut content.operations, index);
                found = true;
            }
            let encoded = content
                .encode()
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?;
            let stream = self
                .inner_mut()
                .get_object_mut(content_id)
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?
                .as_stream_mut()
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?;
            stream.set_plain_content(encoded);
        }
        Ok(found)
    }

    /// Pose la matrice qui dessine une image sur sa page.
    ///
    /// Sert au déplacement et au redimensionnement d'un bloc image : l'origine
    /// se déplace, les vecteurs s'allongent, mais les pixels ne sont jamais
    /// réencodés. L'opération est donc exacte et réversible, à la différence
    /// d'une réécriture par patch.
    pub fn set_image_matrix(&mut self, info: &ImageInfo, matrix: [f32; 6]) -> Result<()> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = info.resource_name.clone();
        let found = self.edit_image_draw(page_id, &name, |operations, index| {
            if let Some(position) = preceding_cm(operations, index) {
                operations[position].operands = matrix_operands(matrix);
            }
        })?;
        if !found {
            return Err(Error::NotFound(format!("matrice de l'image {name}")));
        }
        Ok(())
    }

    /// Réécrit la matrice qui dessine une image pour la tourner ou la
    /// retourner.
    ///
    /// La transformation est appliquée au *repère* : les vecteurs de base
    /// tournent autour du centre de la boîte, l'origine se replace pour que le
    /// centre ne bouge pas. Les pixels ne changent jamais, donc l'opération est
    /// réversible et sans perte.
    fn transform_image_matrix(
        &mut self,
        page: usize,
        resource_name: &str,
        transform: ImageTransform,
    ) -> Result<bool> {
        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = resource_name.to_owned();
        self.edit_image_draw(page_id, &name, |operations, index| {
            let Some(position) = preceding_cm(operations, index) else {
                return;
            };
            let Some([a, b, c, d, e, f]) = matrix_of(&operations[position].operands) else {
                return;
            };
            // Le repère du dessin : `(a, b)` va de gauche à droite de l'image,
            // `(c, d)` du bas vers le haut. `e, f` est le coin bas-gauche.
            let (xa, xb, ya, yb) = match transform {
                // Quart de tour horaire à l'écran : (x, y) devient (y, -x).
                ImageTransform::RotateCw => (b, -a, d, -c),
                ImageTransform::RotateCcw => (-b, a, -d, c),
                // Miroirs : on inverse le vecteur de l'axe concerné.
                ImageTransform::FlipHorizontal => (-a, -b, c, d),
                ImageTransform::FlipVertical => (a, b, -c, -d),
            };
            // Centre conservé : l'origine se déplace de la moitié de l'écart.
            let ne = e + 0.5 * (a + c - xa - ya);
            let nf = f + 0.5 * (b + d - xb - yb);
            operations[position].operands = matrix_operands([xa, xb, ya, yb, ne, nf]);
        })
    }

    /// Rétrécit la boîte de dessin d'une image du côté rogné, sans la
    /// déplacer : le coin bas-gauche local part de `left` / `bottom` et les
    /// vecteurs gardent la part conservée.
    fn crop_image_matrix(
        &mut self,
        page: usize,
        resource_name: &str,
        crop: ImageCrop,
    ) -> Result<bool> {
        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = resource_name.to_owned();
        self.edit_image_draw(page_id, &name, |operations, index| {
            let Some(position) = preceding_cm(operations, index) else {
                return;
            };
            let Some([a, b, c, d, e, f]) = matrix_of(&operations[position].operands) else {
                return;
            };
            let keep_x = (1.0 - crop.left - crop.right).max(0.01);
            let keep_y = (1.0 - crop.top - crop.bottom).max(0.01);
            let ne = e + a * crop.left + c * crop.bottom;
            let nf = f + b * crop.left + d * crop.bottom;
            operations[position].operands =
                matrix_operands([a * keep_x, b * keep_x, c * keep_y, d * keep_y, ne, nf]);
        })
    }
}

/// Opérateurs `cm` de la matrice `[a b c d e f]`.
fn matrix_operands(values: [f32; 6]) -> Vec<Object> {
    values
        .iter()
        .map(|value| Object::Real(*value))
        .collect()
}

/// Lit les six nombres d'un `cm`, ou rien si les opérandes sont inattendus.
fn matrix_of(operands: &[Object]) -> Option<[f32; 6]> {
    if operands.len() != 6 {
        return None;
    }
    let mut values = [0.0f32; 6];
    for (slot, operand) in values.iter_mut().zip(operands) {
        *slot = operand.as_float().ok()?;
    }
    Some(values)
}

/// Position du dernier `cm` avant un `Do`, sans remonter au-delà de la
/// dernière image ou du dernier `q`.
fn preceding_cm(operations: &[lopdf::content::Operation], index: usize) -> Option<usize> {
    for candidate in (0..index).rev() {
        match operations[candidate].operator.as_str() {
            "cm" => return Some(candidate),
            "q" | "Q" | "Do" => return None,
            _ => {}
        }
    }
    None
}

/// Bords conservés d'un rognage, en **pixels** : `(x0, y0, x1, y1)`.
///
/// Les pixels et la matrice du flux de page sont calculés à partir de ces
/// mêmes bornes, sinon ils divergeraient d'une fraction de pixel.
fn crop_bounds(width: u32, height: u32, crop: ImageCrop) -> (u32, u32, u32, u32) {
    let x0 = ((crop.left * width as f32).floor() as u32).min(width.saturating_sub(1));
    let x1 = ((width as f32 * (1.0 - crop.right)).ceil() as u32).clamp(x0 + 1, width);
    let y0 = ((crop.top * height as f32).floor() as u32).min(height.saturating_sub(1));
    let y1 = ((height as f32 * (1.0 - crop.bottom)).ceil() as u32).clamp(y0 + 1, height);
    (x0, y0, x1, y1)
}

/// Découpe les pixels selon des bornes en pixels.
fn crop_pixels(
    pixels: &[u8],
    width: u32,
    height: u32,
    (x0, y0, x1, y1): (u32, u32, u32, u32),
) -> (Vec<u8>, u32, u32) {
    let out_width = x1.saturating_sub(x0).max(1).min(width);
    let out_height = y1.saturating_sub(y0).max(1).min(height);
    let mut out = vec![0u8; out_width as usize * out_height as usize * 3];
    for y in 0..out_height {
        for x in 0..out_width {
            let sx = (x0 + x).min(width - 1);
            let sy = (y0 + y).min(height - 1);
            let source = (sy as usize * width as usize + sx as usize) * 3;
            let target = (y as usize * out_width as usize + x as usize) * 3;
            if source + 3 <= pixels.len() {
                out[target..target + 3].copy_from_slice(&pixels[source..source + 3]);
            }
        }
    }
    (out, out_width, out_height)
}

// ────────────────────────────────────────────────────────────────────────
// Aides internes
// ────────────────────────────────────────────────────────────────────────

/// Découpe grossièrement un flux de contenu en tokens.
///
/// Reconnaît : noms (`/X`), chaînes `(…)`, tableaux `[…]`, nombres,
/// opérateurs. Ne gère pas les chaînes hexadécimales `<…>` (rare pour
/// du texte).
fn tokenize_content(content: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(content);
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        // Chaîne littérale (…)
        if c == '(' {
            chars.next();
            let mut depth = 1;
            let mut s = String::new();
            while let Some(&nc) = chars.peek() {
                chars.next();
                match nc {
                    '\\' => {
                        // Caractère d'échappement : on le consomme et on
                        // ajoute le suivant.
                        if let Some(&escaped) = chars.peek() {
                            chars.next();
                            match escaped {
                                'n' => s.push('\n'),
                                'r' => s.push('\r'),
                                't' => s.push('\t'),
                                other => s.push(other),
                            }
                        }
                    }
                    '(' => {
                        depth += 1;
                        s.push('(');
                    }
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        s.push(')');
                    }
                    other => s.push(other),
                }
            }
            tokens.push(format!("({s})"));
            continue;
        }

        // Nom /X
        if c == '/' {
            chars.next();
            let mut s = String::from("/");
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() || nc == '/' || nc == '(' || nc == '[' || nc == ']' {
                    break;
                }
                s.push(nc);
                chars.next();
            }
            tokens.push(s);
            continue;
        }

        // Délimiteur d'array : un jeton à part entière. Sans ce cas, le `[`
        // ou le `]` serait relu sans fin — la boucle extérieure le repère, la
        // boucle « Opérateur ou nombre » le rejette sans l'avancer.
        if c == '[' || c == ']' {
            chars.next();
            tokens.push(c.to_string());
            continue;
        }

        // Opérateur ou nombre
        let mut s = String::new();
        while let Some(&nc) = chars.peek() {
            if nc.is_whitespace() || nc == '(' || nc == '/' || nc == '[' || nc == ']' {
                break;
            }
            s.push(nc);
            chars.next();
        }
        if !s.is_empty() {
            tokens.push(s);
        }
    }

    tokens
}

/// Extrait le contenu d'un token de type `(…)` s'il y en a un.
fn extract_string_literal(token: &str) -> Option<String> {
    if token.starts_with('(') && token.ends_with(')') {
        Some(token[1..token.len() - 1].to_string())
    } else {
        None
    }
}

/// Longueur du dernier fragment poussé (pour l'avance horizontale).
fn current_text_len(fragments: &[TextFragment]) -> usize {
    fragments
        .last()
        .map(|f| f.text.chars().count())
        .unwrap_or(0)
}

/// Pousse ou accumule un fragment de texte.
#[allow(clippy::too_many_arguments)]
fn push_fragment(
    fragments: &mut Vec<TextFragment>,
    current: &mut Option<TextFragment>,
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) {
    if text.trim().is_empty() {
        return;
    }

    match current {
        Some(frag) => {
            // On continue le fragment courant : le texte suit sur la
            // même ligne (même ty à la tolérance près).
            if (frag.rect[1] - ty).abs() < font_size * 0.3 {
                frag.text.push_str(&text);
                // On étend le rectangle.
                frag.rect[2] += font_size * 0.5 * text.chars().count() as f32;
            } else {
                fragments.push(frag.clone());
                *frag = new_fragment(text, page, tx, ty, font_size, font_name);
            }
        }
        None => {
            *current = Some(new_fragment(text, page, tx, ty, font_size, font_name));
        }
    }
}

fn new_fragment(
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) -> TextFragment {
    let width = font_size * 0.5 * text.chars().count() as f32;
    TextFragment {
        page,
        text,
        rect: [tx, ty, tx + width, ty + font_size],
        font: font_name,
        font_size,
        color: [0, 0, 0],
    }
}

// ────────────────────────────────────────────────────────────────────────
// Remplacement de texte par « patch »
// ────────────────────────────────────────────────────────────────────────

impl Document {
    /// Remplace un fragment de texte par un nouveau contenu.
    ///
    /// `background` est la couleur du rectangle de masquage (composantes
    /// 0.0–1.0) ; `None` donne du blanc. Un appelant qui connaît la couleur
    /// réelle de la page doit la fournir, sinon le patch se voit sur une
    /// page teintée.
    ///
    /// ## Méthode
    ///
    /// 1. On dessine un rectangle de masquage sur l'ancien fragment.
    /// 2. On écrit le nouveau texte au même emplacement, avec Helvetica,
    ///    dans la couleur du fragment (`TextFragment::color`).
    ///
    /// ## Limites
    ///
    /// - La police d'origine n'est pas reproduite : on utilise
    ///   Helvetica, l'une des 14 polices standard PDF.
    /// - Le style (gras, italique) n'est pas préservé.
    /// - Le texte n'est pas ré-encodable dans le flux d'origine : il est
    ///   écrit par-dessus. Le contenu original reste présent dans le PDF
    ///   (récupérable par extraction), ce qui **doit** être su si le but
    ///   est de supprimer une information sensible (utiliser le
    ///   caviardage pour ça).
    pub fn replace_text_fragment(&mut self, fragment: &TextFragment, new_text: &str, background: Option<[f32; 3]>) -> Result<()> {
        let style = TextStyle::from_fragment(fragment);
        self.replace_text_fragment_styled(fragment, new_text, background, &style)
    }

    /// Remplace un fragment de texte avec une mise en forme choisie.
    ///
    /// Mêmes masquage et emplacement que [`Document::replace_text_fragment`] ;
    /// la police, le corps, la couleur, les styles (gras, italique, souligné,
    /// barré), l'alignement, l'interligne et l'approche viennent de `style`.
    /// Les lignes sont séparées par `\n`. La police reste l'une des quatorze
    /// standard : le patch ne peut pas reprendre la police d'origine.
    pub fn replace_text_fragment_styled(
        &mut self,
        fragment: &TextFragment,
        new_text: &str,
        background: Option<[f32; 3]>,
        style: &TextStyle,
    ) -> Result<()> {
        self.replace_text_fragment_at(fragment, new_text, fragment.rect, background, style)
    }

    /// Réécrit un fragment de texte à un autre emplacement.
    ///
    /// `target` est la boîte où poser le nouveau texte, en points ; le fragment
    /// d'origine est masqué comme pour une réécriture sur place. C'est ce qui
    /// déplace ou redimensionne un bloc : le texte est réécrit, il n'est pas
    /// translaté dans le flux, donc la police d'origine est perdue (voir les
    /// limites de [`Document::replace_text_fragment`]).
    pub fn replace_text_fragment_at(
        &mut self,
        fragment: &TextFragment,
        new_text: &str,
        target: [f32; 4],
        background: Option<[f32; 3]>,
        style: &TextStyle,
    ) -> Result<()> {
        let page_id = self
            .page_id(fragment.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // 1. Préparer le flux de patch.
        let [x1, y1, x2, y2] = target;
        let w = x2 - x1;

        // Le flux est construit en **octets** : un `String` intermédiaire puis
        // `into_bytes()` ré-encoderait chaque octet WinAnsi
        // >= 0x80 en deux octets UTF-8, ce qui reproduirait exactement le
        // défaut d'affichage des accents que ce correctif supprime.
        let mut content: Vec<u8> = Vec::new();

        // 2. Géométrie des lignes. La première garde la ligne de base du
        //    fragment d'origine ; les suivantes descendent d'un interligne.
        //    Le masque couvre donc tout ce qui sera écrit, sinon la seconde
        //    ligne se poserait sur le contenu de la page.
        let size = style.size.clamp(MIN_TEXT_SIZE, MAX_TEXT_SIZE);
        let ink = [
            style.color[0] as f32 / 255.0,
            style.color[1] as f32 / 255.0,
            style.color[2] as f32 / 255.0,
        ];
        let base = style.font.base_font(style.bold, style.italic);
        let font_name = patch_font_name(base);
        let line_step = size * style.line_spacing.max(0.5);
        let first_baseline = y1 + size * 0.2;
        // Un `\r` de fin de ligne (texte venu d'un champ CRLF) ne fait pas
        // partie du texte : il serait écrit comme une espace.
        let lines: Vec<&str> = new_text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect();
        let line_count = lines.len().max(1) as f32;
        let last_baseline = first_baseline - (line_count - 1.0) * line_step;
        let mask_bottom = (last_baseline - size * 0.3).min(y1) - 1.0;

        // Rectangle de masquage : couleur contrôlée, par défaut blanc. Il
        // couvre l'emplacement d'origine **et** le nouveau : sans cela, un bloc
        // déplacé laisserait son ancien texte derrière lui. Quand les deux
        // coïncident, la réunion redonne exactement l'ancien rectangle.
        let bg = background.unwrap_or([1.0, 1.0, 1.0]);
        let [ox1, oy1, ox2, oy2] = fragment.rect;
        let mask_x1 = (ox1 - 1.0).min(x1 - 1.0);
        let mask_y1 = (oy1 - 1.0).min(mask_bottom);
        let mask_x2 = (ox2 + 1.0).max(x2 + 1.0);
        let mask_y2 = (oy2 + 1.0).max(y2 + 1.0);
        content.extend_from_slice(
            format!("q\n{:.2} {:.2} {:.2} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                bg[0],
                bg[1],
                bg[2],
                mask_x1,
                mask_y1,
                mask_x2 - mask_x1,
                (mask_y2 - mask_y1).max(1.0))
            .as_bytes(),
        );

        // 3. Texte de remplacement. La couleur est reposée explicitement :
        //    après le `Q` du masque, la couleur de remplissage courante est
        //    celle qu'avait laissée le contenu de la page, qui n'a rien à voir.
        for (line_index, line) in lines.iter().enumerate() {
            let width = estimate_text_width(line, style);
            let dx = match style.align {
                TextAlign::Left | TextAlign::Justify => 0.0,
                TextAlign::Center => ((w - width) / 2.0).max(0.0),
                TextAlign::Right => (w - width).max(0.0),
            };
            let baseline = first_baseline - line_index as f32 * line_step;
            let x = x1 + dx;
            // L'approche n'est écrite que si elle est demandée : un patch sans
            // réglage garde le flux qu'il produisait avant l'inspecteur.
            let spacing = if style.char_spacing == 0.0 {
                String::new()
            } else {
                format!("{:.2} Tc\n", style.char_spacing)
            };
            // Justification : les mots sont répartis sur la largeur de la
            // boîte. Une ligne d'un seul mot, ou plus large que la boîte,
            // reste à gauche.
            let words: Vec<&str> = line.split(' ').filter(|word| !word.is_empty()).collect();
            let justify =
                style.align == TextAlign::Justify && words.len() > 1 && width < w;
            content.extend_from_slice(
                format!(
                    "BT\n{:.3} {:.3} {:.3} rg\n/{} {:.2} Tf\n{spacing}",
                    ink[0], ink[1], ink[2], font_name, size
                )
                .as_bytes(),
            );
            // Largeur réellement couverte : toute la boîte quand la ligne est
            // justifiée, la largeur estimée sinon.
            let rule_width = if justify {
                let natural: f32 = words
                    .iter()
                    .map(|word| estimate_text_width(word, style))
                    .sum();
                let gap = ((w - natural) / (words.len() - 1) as f32).max(0.0);
                let mut cursor = x;
                for (index, word) in words.iter().enumerate() {
                    if index > 0 {
                        cursor += gap;
                    }
                    content.extend_from_slice(
                        format!("1 0 0 1 {cursor:.2} {baseline:.2} Tm\n").as_bytes(),
                    );
                    content.push(b'(');
                    content.extend_from_slice(&encode_pdf_literal(word));
                    content.extend_from_slice(b") Tj\n");
                    cursor += estimate_text_width(word, style);
                }
                w
            } else {
                content.extend_from_slice(
                    format!("{x:.2} {baseline:.2} Td\n").as_bytes(),
                );
                content.push(b'(');
                content.extend_from_slice(&encode_pdf_literal(line));
                content.extend_from_slice(b") Tj\n");
                width
            };
            content.extend_from_slice(b"ET\n");

            // Souligné et barré se dessinent : le format texte de PDF n'a pas
            // de trait sous le glyphe. Une ligne vide n'a rien à souligner :
            // un trait d'un point trahirait le patch.
            let rule = (size * 0.07).max(0.5);
            let ruled = width > 0.1;
            if style.underline && ruled {
                let y = baseline - size * 0.14;
                content.extend_from_slice(
                    format!(
                        "q\n{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                        ink[0], ink[1], ink[2], x, y, rule_width.max(1.0), rule
                    )
                    .as_bytes(),
                );
            }
            if style.strikeout && ruled {
                let y = baseline + size * 0.28;
                content.extend_from_slice(
                    format!(
                        "q\n{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\nQ\n",
                        ink[0], ink[1], ink[2], x, y, rule_width.max(1.0), rule
                    )
                    .as_bytes(),
                );
            }
        }

        let patch = Stream::new(Dictionary::new(), content);
        let patch_id = self.inner_mut().add_object(patch);

        // 3. Police dédiée aux patchs (police standard + WinAnsi), indépendante
        //    des polices déjà présentes sur la page.
        self.ensure_patch_font(page_id, base)?;

        // 4. Concaténer le patch au flux de contenu.
        self.append_to_page_content(page_id, patch_id)?;

        Ok(())
    }

    /// Garantit qu'une police standard est disponible dans le document.
    ///
    /// Le nom de ressource dépend du fichier de base : chaque variante
    /// (gras, italique, famille) garde la sienne, sinon la seconde écraserait
    /// la première. Réutilise la police si elle est déjà présente.
    /// Nom de ressource de la police posée par les patchs YouPDF.
    ///
    /// On n'utilise **jamais** le `/F1` de la page : cette police peut être
    /// une Typ1 sans `/Encoding` (StandardEncoding), une police sous-ensemblée
    /// en `Identity-H` (UTF-16) ou toute autre encodage. Y écrire des octets
    /// WinAnsi produit du texte illisible. Une police dédiée garantit
    /// l'encodage et n'écrase rien.
    fn ensure_patch_font(&mut self, page_id: (u32, u16), base_font: &str) -> Result<(u32, u16)> {
        // Nom libre, ou nom déjà pris par **la même** variante. Le nom seul ne
        // suffit pas : un document peut porter `/YouPdfF1` sur un autre fichier
        // de base (patch d'une version antérieure, outil tiers). Dans ce cas on
        // prend un nom suffixé plutôt que d'écraser une police dont d'autres
        // textes se servent.
        let base_name = patch_font_name(base_font);
        let mut name = base_name.clone();
        let mut suffix = 1u32;
        loop {
            let existing = self
                .inner()
                .get_dictionary(page_id)
                .ok()
                .and_then(|page| {
                    page.get(b"Resources")
                        .ok()
                        .and_then(|resources| resources.as_dict().ok())
                })
                .and_then(|resources| {
                    resources
                        .get(b"Font")
                        .ok()
                        .and_then(|fonts| fonts.as_dict().ok())
                })
                .and_then(|fonts| fonts.get(name.as_bytes()).ok())
                .and_then(|object| object.as_reference().ok());
            let Some(id) = existing else {
                break;
            };
            let matching = self
                .inner()
                .get_dictionary(id)
                .ok()
                .and_then(|font| {
                    font.get(b"BaseFont")
                        .ok()
                        .and_then(|base| base.as_name().ok())
                        .map(|base| base == base_font.as_bytes())
                })
                .unwrap_or(false);
            if matching {
                return Ok(id);
            }
            suffix += 1;
            name = format!("{base_name}{suffix}");
        }

        let font = Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(base_font.as_bytes().to_vec())),
            ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        let font_id = self.inner_mut().add_object(Object::Dictionary(font));
        self.link_font_to_page(page_id, &name, font_id)?;
        Ok(font_id)
    }

    /// Enregistre une police dans les ressources `/Font` d'une page.
    fn link_font_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        font_id: (u32, u16),
    ) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // Récupère ou crée /Resources.
        // (On clone d'abord l'objet pour libérer l'emprunt mutable sur
        // la page, ce qui permet ensuite d'accéder à `self.inner()`.)
        let resources_obj = dict.get(b"Resources").ok().cloned();
        let mut resources = match resources_obj {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(id)) => self
                .inner()
                .get_dictionary(id)
                .map_err(|e| Error::Pdf(format!("Ressources : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Récupère ou crée /Font.
        let mut fonts = match resources.get(b"Font") {
            Ok(Object::Dictionary(d)) => d.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("Font : {e}")))?
                .clone(),
            _ => Dictionary::new(),
        };

        // Associe le nom à la police.
        fonts.set(name, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));

        // Écrit dans la page.
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", Object::Dictionary(resources));

        Ok(())
    }

    /// Concatène un flux à la fin du contenu d'une page.
    fn append_to_page_content(&mut self, page_id: (u32, u16), patch_id: (u32, u16)) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                // Contenu unique : on transforme en tableau.
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(patch_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => {
                arr.push(Object::Reference(patch_id));
            }
            _ => {
                dict.set("Contents", Object::Reference(patch_id));
            }
        }

        Ok(())
    }
}

/// Échappe une chaîne et l'encode en octets WinAnsi pour un littéral PDF `(…)`.
///
/// La page de codes est celle de la police posée (`WinAnsiEncoding`) : il
/// faut donc écrire **un octet** par caractère. Renvoyer une `String` et la
/// convertir en UTF-8 produisait deux octets pour chaque accentué — « café »
/// sortait en `caf\xc3\xa9`, que le lecteur affiche « cafÃ© ».
/// Décode une image disque (PNG ou JPEG) en pixels RGB 8 bits.
///
/// Rend `(pixels, largeur, hauteur)`, prêt pour
/// [`Document::insert_image_bitmap`].
pub fn load_image_rgb(
    path: impl AsRef<std::path::Path>,
) -> Result<(Vec<u8>, u32, u32)> {
    let path = path.as_ref();
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
    // Plafonds explicites : une image « bombe » (dimensions énormes annoncées
    // dans l'en-tête) est refusée au décodage plutôt qu'après allocation.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .with_guessed_format()
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?
        .decode()
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
    let rgb = image.to_rgb8();
    let (width, height) = rgb.dimensions();
    Ok((rgb.into_raw(), width, height))
}

pub fn encode_pdf_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' | '\r' | '\t' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Hors Latin-1/CP1252 (CJK, cyrillique …) : la police standard
            // n'a pas de glyphe garanti → point d'interrogation plutôt
            // qu'un octet arbitraire rendu avec un glyphe au hasard.
            c => out.push(winansi_byte(c).unwrap_or(b'?')),
        }
    }
    out
}

/// Convertit un caractère en octet WinAnsi, avec les écarts usuels entre
/// Latin-1 et CP1252 (guillemets, tirets, points de suspension…).
pub(crate) fn winansi_byte_pub(c: char) -> Option<u8> {
    winansi_byte(c)
}

fn winansi_byte(c: char) -> Option<u8> {
    const CP1252: [(char, u8); 27] = [
        ('\u{20AC}', 0x80), ('\u{201A}', 0x82), ('\u{0192}', 0x83), ('\u{201E}', 0x84),
        ('\u{2026}', 0x85), ('\u{2020}', 0x86), ('\u{2021}', 0x87), ('\u{02C6}', 0x88),
        ('\u{2030}', 0x89), ('\u{0160}', 0x8A), ('\u{2039}', 0x8B), ('\u{0152}', 0x8C),
        ('\u{017D}', 0x8E), ('\u{2018}', 0x91), ('\u{2019}', 0x92), ('\u{201C}', 0x93),
        ('\u{201D}', 0x94), ('\u{2022}', 0x95), ('\u{2013}', 0x96), ('\u{2014}', 0x97),
        ('\u{02DC}', 0x98), ('\u{2122}', 0x99), ('\u{0161}', 0x9A), ('\u{203A}', 0x9B),
        ('\u{0153}', 0x9C), ('\u{017E}', 0x9E), ('\u{0178}', 0x9F),
    ];
    if let Some((_, b)) = CP1252.iter().find(|(ch, _)| *ch == c) {
        return Some(*b);
    }
    let u = c as u32;
    if (0xA0..=0xFF).contains(&u) {
        Some(u as u8)
    } else {
        None
    }
}
