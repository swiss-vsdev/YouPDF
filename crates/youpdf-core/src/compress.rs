//! Compression et optimisation du document.
//!
//! `Document::compress` ré-écrit le document à un emplacement et renvoie
//! les tailles avant/après, en octets. Trois opérations sont effectuées,
//! via `lopdf` 0.34 :
//!
//! - recompression des flux en `FlateDecode` (`Document::compress`),
//! - suppression des objets que l'arbre ne référence plus
//!   (`Document::prune_objects`),
//! - resserrement des identifiants d'objets (`Document::renumber_objects`).
//!
//! `Document::compress_with` ajoute un passage sur les images : chaque image
//! `DeviceRGB`/`DeviceGray` de 8 bits est rééchantillonnée à la résolution
//! cible du niveau choisi, puis ré-encodée en JPEG. C'est ce qui fait maigrir
//! un document numérisé.
//!
//! ## Ce que cette compression ne fait **pas**
//!
//! - Pas d'*object streams* : `lopdf` 0.34 sait les **lire**, pas les
//!   écrire, donc le fichier écrit reste en objets « classiques ».
//! - Pas de sous-ensemble de polices (font subsetting), pas de
//!   déduplication des ressources de page.
//! - Les images masquées, CMJN, à prédicteur, ou non-JPEG/Flate sont laissées
//!   intactes.
//!
//! Le gain dépend donc du document ; un PDF déjà optimisé peut maigrir peu
//! ou même grossir légèrement.

use std::io::Read;
use std::path::Path;

use flate2::read::ZlibDecoder;
use lopdf::Document as LopdfDocument;
use youpdf_shared::Result;

use crate::document::Document;

/// Niveau de compression d'image.
///
/// Chaque niveau associe une résolution DPI cible et une qualité JPEG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// Écran (~72 dpi), qualité JPEG 55. Pour un document lu à l'écran.
    Screen,
    /// eBook (~150 dpi), qualité JPEG 75. Pour une liseuse ou une tablette.
    Ebook,
    /// Impression (~300 dpi), qualité JPEG 88. Pour une impression papier.
    Print,
}

impl CompressionLevel {
    /// Tous les niveaux, dans l'ordre d'affichage.
    pub const ALL: [CompressionLevel; 3] = [
        CompressionLevel::Screen,
        CompressionLevel::Ebook,
        CompressionLevel::Print,
    ];

    /// Nom lisible du niveau (ex : « Screen (72 dpi) »).
    pub fn display_name(self) -> &'static str {
        match self {
            CompressionLevel::Screen => "Screen (72 dpi)",
            CompressionLevel::Ebook => "Ebook (150 dpi)",
            CompressionLevel::Print => "Print (300 dpi)",
        }
    }

    /// Options associées à ce niveau.
    pub fn options(self) -> CompressionOptions {
        match self {
            CompressionLevel::Screen => CompressionOptions {
                target_dpi: 72.0,
                jpeg_quality: 55,
                upscale: false,
            },
            CompressionLevel::Ebook => CompressionOptions {
                target_dpi: 150.0,
                jpeg_quality: 75,
                upscale: false,
            },
            CompressionLevel::Print => CompressionOptions {
                target_dpi: 300.0,
                jpeg_quality: 88,
                upscale: false,
            },
        }
    }
}

/// Options de compression d'image.
#[derive(Debug, Clone)]
pub struct CompressionOptions {
    /// DPI cible pour le rééchantillonnage des images.
    pub target_dpi: f32,
    /// Qualité JPEG, de 1 à 100.
    pub jpeg_quality: u8,
    /// Si `true`, les images plus petites que la cible sont agrandies elles
    /// aussi. Faux par défaut : on n'ajoute jamais de pixels.
    pub upscale: bool,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        CompressionLevel::Ebook.options()
    }
}

/// Bilan d'une compression : tailles avant et après, en octets, plus le nombre
/// d'images rééchantillonnées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionReport {
    /// Taille de la sérialisation en mémoire **avant** optimisation.
    pub before_bytes: usize,
    /// Taille du fichier écrit **après** optimisation.
    pub after_bytes: usize,
    /// Nombre d'images rééchantillonnées et ré-encodées en JPEG.
    pub images_resampled: usize,
}

impl CompressionReport {
    /// Octets économisés (positif) ou ajoutés (négatif).
    pub fn saved_bytes(&self) -> isize {
        self.before_bytes as isize - self.after_bytes as isize
    }
}

impl Document {
    /// Compresse le document et l'écrit à `path`.
    ///
    /// Passage structurel seulement : voir [`Document::compress_with`] pour
    /// rééchantillonner aussi les images. La taille peut augmenter sur un
    /// document déjà optimisé.
    pub fn compress(&mut self, path: impl AsRef<Path>) -> Result<CompressionReport> {
        let path = path.as_ref();
        let before_bytes = self.to_bytes()?.len();

        // `inner_mut` (et non `&mut self.inner`) : il invalide le cache de
        // sérialisation. Sans cela, `save` réécrirait les octets d'avant.
        {
            let inner = self.inner_mut();
            inner.prune_objects();
            inner.renumber_objects();
            inner.compress();
        }

        self.save(path)?;

        let after_bytes = std::fs::metadata(path)?.len() as usize;
        Ok(CompressionReport {
            before_bytes,
            after_bytes,
            images_resampled: 0,
        })
    }

    /// Compresse le document avec un niveau donné.
    ///
    /// 1. Rééchantillonne et ré-encode les images selon `options`.
    /// 2. Exécute le passage structurel (`FlateDecode`, élagage, renumérotation).
    /// 3. Écrit le document à `path` et retourne le bilan.
    pub fn compress_with(
        &mut self,
        path: impl AsRef<Path>,
        options: &CompressionOptions,
    ) -> Result<CompressionReport> {
        let path = path.as_ref();
        let before_bytes = self.to_bytes()?.len();

        let images_resampled = compress_images(self.inner_mut(), options)?;

        {
            let inner = self.inner_mut();
            inner.prune_objects();
            inner.renumber_objects();
            inner.compress();
        }

        self.save(path)?;

        let after_bytes = std::fs::metadata(path)?.len() as usize;
        Ok(CompressionReport {
            before_bytes,
            after_bytes,
            images_resampled,
        })
    }
}

/// Rééchantillonne et ré-encode en JPEG les images du document.
///
/// Retourne le nombre d'images remplacées.
///
/// Approche : on copie chaque stream, on le traite hors du dictionnaire, puis
/// on le réinsère seulement si le JPEG obtenu est plus petit. Le stream reste
/// donc intact quand la recompression ne gagne rien.
fn compress_images(
    document: &mut LopdfDocument,
    options: &CompressionOptions,
) -> Result<usize> {
    // Un DPI cible nul ou négatif désactive le passage image.
    if options.target_dpi <= 0.0 {
        return Ok(0);
    }

    let max_width = largest_page_width(document);
    let target_px_width = (max_width / 72.0 * options.target_dpi).max(1.0) as u32;

    // Copie des identifiants : le dictionnaire est muté pendant la boucle.
    let object_ids: Vec<lopdf::ObjectId> = document.objects.keys().copied().collect();
    let mut count = 0usize;

    for object_id in object_ids {
        let mut stream = match document
            .objects
            .get(&object_id)
            .and_then(|o| o.as_stream().ok().cloned())
        {
            Some(s) => s,
            None => continue,
        };

        // Masque (1 bit) : ce n'est pas une image à rééchantillonner.
        if stream
            .dict
            .get(b"ImageMask")
            .and_then(|o| o.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        // Seulement 8 bits par composant.
        if stream
            .dict
            .get(b"BitsPerComponent")
            .and_then(|o| o.as_i64())
            .ok()
            != Some(8)
        {
            continue;
        }

        // Seulement RGB ou gris.
        let cs_name = match stream
            .dict
            .get(b"ColorSpace")
            .and_then(|o| o.as_name_str())
        {
            Ok(name) => name.to_string(),
            Err(_) => continue,
        };
        if cs_name != "DeviceRGB" && cs_name != "DeviceGray" {
            continue;
        }

        // Un prédicteur (`/DecodeParms`) change la disposition des échantillons :
        // on ne le gère pas.
        if stream.dict.get(b"DecodeParms").is_ok() || stream.dict.get(b"DP").is_ok() {
            continue;
        }

        // Un seul filtre, JPEG ou Flate, ou pas de filtre du tout.
        let filters = stream.filters().unwrap_or_default();
        let acceptable = filters.is_empty()
            || (filters.len() == 1
                && (filters[0] == "DCTDecode" || filters[0] == "FlateDecode"));
        if !acceptable {
            continue;
        }

        let img = match decode_stream_content(&stream) {
            Some(img) => img,
            None => continue,
        };

        let orig_width = img.width();
        let orig_height = img.height();
        if orig_width == 0 || orig_height == 0 {
            continue;
        }

        let target_w = target_px_width.max(1);
        let target_h =
            ((orig_height as f32 / orig_width as f32) * target_w as f32).max(1.0) as u32;

        let ratio_x = target_w as f32 / orig_width as f32;
        let ratio_y = target_h as f32 / orig_height as f32;
        if ratio_x.max(ratio_y) >= 1.0 && !options.upscale {
            continue;
        }

        let scale = if options.upscale {
            ratio_x.max(ratio_y)
        } else {
            ratio_x.min(ratio_y)
        };
        let new_w = ((orig_width as f32 * scale).round() as u32).max(1);
        let new_h = ((orig_height as f32 * scale).round() as u32).max(1);

        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);

        // Garder le gris en gris : c'est trois fois moins d'octets.
        let (samples, color_type) = if resized.color() == image::ColorType::L8 {
            (resized.to_luma8().into_raw(), image::ExtendedColorType::L8)
        } else {
            (resized.to_rgb8().into_raw(), image::ExtendedColorType::Rgb8)
        };

        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg,
            options.jpeg_quality.clamp(1, 100),
        );
        encoder
            .encode(&samples, new_w, new_h, color_type)
            .map_err(|e| youpdf_shared::Error::Pdf(format!("encodage JPEG image : {e}")))?;

        // Ne remplacer que si le résultat est plus petit.
        if jpeg.len() >= stream.content.len() {
            continue;
        }

        stream.dict.remove(b"Filter");
        stream.dict.remove(b"DecodeParms");
        stream.dict.remove(b"DP");
        stream.dict.set(b"Filter", "DCTDecode");
        stream.dict.set(b"Width", lopdf::Object::Integer(new_w as i64));
        stream.dict.set(b"Height", lopdf::Object::Integer(new_h as i64));
        stream.dict.set(b"BitsPerComponent", lopdf::Object::Integer(8));
        let gray = color_type == image::ExtendedColorType::L8;
        stream.dict.set(
            b"ColorSpace",
            lopdf::Object::Name(if gray {
                b"DeviceGray".to_vec()
            } else {
                b"DeviceRGB".to_vec()
            }),
        );
        stream.set_content(jpeg);

        // Réinsérer : sans cette écriture, tout le travail serait perdu.
        document.objects.insert(object_id, lopdf::Object::Stream(stream));
        count += 1;
    }

    Ok(count)
}

/// Largeur de page la plus grande du document, en points.
///
/// Sert de référence pour convertir le DPI cible en pixels. 595 pt (A4) par
/// défaut quand aucune page n'est lisible.
fn largest_page_width(document: &LopdfDocument) -> f32 {
    let mut max_width = 595.0f32;
    for page_id in document.get_pages().values() {
        let Ok(dict) = document.get_dictionary(*page_id) else {
            continue;
        };
        let Ok(array) = dict.get(b"MediaBox").and_then(|o| o.as_array()) else {
            continue;
        };
        if array.len() < 4 {
            continue;
        }
        if let Ok(w) = array[2].as_float() {
            if w > max_width {
                max_width = w;
            }
        }
    }
    max_width
}

/// Décode le contenu d'un stream image.
///
/// Gère le JPEG direct (`DCTDecode`), le zlib d'échantillons bruts
/// (`FlateDecode`), et le contenu brut (pas de filtre). Retourne `None` pour
/// tout ce qui n'est pas une image 8 bits RGB/gris exploitable.
pub(crate) fn decode_stream_content(stream: &lopdf::Stream) -> Option<image::DynamicImage> {
    // Un stream sans `/Filter` fait échouer `filters()` : ce n'est pas une
    // erreur, c'est du contenu brut.
    let filters = stream.filters().unwrap_or_default();

    if filters.is_empty() {
        return raw_samples_to_image(stream, &stream.content);
    }

    if filters.iter().any(|f| f == "DCTDecode") {
        return image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg).ok();
    }

    if filters.iter().any(|f| f == "FlateDecode") {
        // Un flux peut se décompresser en plusieurs gigaoctets (« bombe ») :
        // on plafonne la sortie avant de décider quoi que ce soit.
        const MAX_RAW_BYTES: u64 = 256 * 1024 * 1024;
        let mut decoder =
            ZlibDecoder::new(stream.content.as_slice()).take(MAX_RAW_BYTES);
        let mut raw = Vec::new();
        if decoder.read_to_end(&mut raw).is_err() {
            return None;
        }
        if raw.len() as u64 >= MAX_RAW_BYTES {
            return None;
        }
        // Certains producteurs emballent un JPEG ou un PNG dans un flux Flate.
        if let Ok(img) = image::load_from_memory_with_format(&raw, image::ImageFormat::Jpeg) {
            return Some(img);
        }
        if let Ok(img) = image::load_from_memory_with_format(&raw, image::ImageFormat::Png) {
            return Some(img);
        }
        return raw_samples_to_image(stream, &raw);
    }

    None
}

/// Reconstruit une image à partir d'échantillons bruts, 8 bits par composant.
///
/// `raw` doit contenir au moins `largeur × hauteur × composants` octets ; le
/// surplus (bourrage de ligne) est ignoré.
fn raw_samples_to_image(
    stream: &lopdf::Stream,
    raw: &[u8],
) -> Option<image::DynamicImage> {
    let width = stream.dict.get(b"Width").ok()?.as_i64().ok()?;
    let height = stream.dict.get(b"Height").ok()?.as_i64().ok()?;
    // Une dimension négative viendrait d'un PDF malformé : `as u32` la
    // transformerait en valeur énorme et l'allocation suivrait.
    if width <= 0 || height <= 0 {
        return None;
    }
    let (width, height) = (width as u32, height as u32);
    let color_space = stream.dict.get(b"ColorSpace").ok()?.as_name_str().ok()?;
    let pixels = width as usize * height as usize;

    match color_space {
        "DeviceGray" => {
            if raw.len() < pixels {
                return None;
            }
            let buf = image::GrayImage::from_raw(width, height, raw[..pixels].to_vec())?;
            Some(image::DynamicImage::ImageLuma8(buf))
        }
        "DeviceRGB" => {
            if raw.len() < pixels * 3 {
                return None;
            }
            let buf = image::RgbImage::from_raw(width, height, raw[..pixels * 3].to_vec())?;
            Some(image::DynamicImage::ImageRgb8(buf))
        }
        _ => None,
    }
}