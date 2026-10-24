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
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::{TextChar, TextFragment};

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

/// Bibliothèque pdfium du processus.
///
/// pdfium s'initialise **une fois** pour tout le processus : un second appel
/// à `FPDF_InitLibrary` laisserait le mutex global de `pdfium-render`
/// verrouillé, et le processus entier se figerait (vérifié : deux
/// `PdfiumRenderer` vivants simultanément bloquaient indéfiniment). Les
/// renderers partagent donc cette instance unique, jamais libérée.
static PDFIUM: OnceLock<std::result::Result<Pdfium, String>> = OnceLock::new();

/// Sérialise les appels à pdfium.
///
/// L'option `thread_safe` de `pdfium-render` ne verrouille que
/// `FPDF_InitLibrary` / `FPDF_DestroyLibrary` : les autres appels ne sont
/// pas protégés. Ce verrou global prend le relais pour tous les renderers
/// (fil d'interface, fil d'arrière-plan, tests), qui peuvent être sur des
/// threads différents même si chacun reste `!Sync`.
static PDFIUM_LOCK: Mutex<()> = Mutex::new(());

/// Emprunte le mutex global des appels pdfium.
///
/// Un thread tombé en panique peut avoir empoisonné le verrou : l'état
/// qu'il protège n'est qu'un jeton d'exclusion, on le récupère tel quel.
fn pdfium_lock() -> MutexGuard<'static, ()> {
    PDFIUM_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Charge (une seule fois) la bibliothèque pdfium du processus.
fn shared_pdfium() -> Result<&'static Pdfium> {
    let loaded = PDFIUM.get_or_init(|| {
        let bindings = PdfiumRenderer::bind_pdfium().map_err(|e| e.to_string())?;
        tracing::info!("pdfium chargé avec succès");
        Ok(Pdfium::new(bindings))
    });
    match loaded {
        Ok(pdfium) => Ok(pdfium),
        Err(msg) => Err(Error::Config(msg.clone())),
    }
}

/// Implémentation pdfium.
///
/// pdfium n'est **pas** thread-safe : le parseur en cache est protégé par un
/// `RefCell` et `PdfiumRenderer` n'est donc **pas** `Sync`. On l'utilise
/// depuis le fil d'interface ou depuis un fil de travail, mais les appels à
/// pdfium passent tous par [`PDFIUM_LOCK`].
///
/// Le document parsé par pdfium est conservé d'un appel à l'autre : le
/// reparsing complet à chaque page rendue coûtait O(taille du document) par
/// page, donc O(n²) pour un document de n pages. La clé du cache est le
/// couple (identité du document, génération) : deux `Document` distincts
/// partent tous deux de la génération 0.
pub struct PdfiumRenderer {
    pdfium: &'static Pdfium,
    parsed: RefCell<Option<((u64, u64), PdfDocument<'static>)>>,
    /// Nombre de parsings effectués (tests : le cache doit servir).
    #[cfg(test)]
    parses: Cell<u64>,
}

impl PdfiumRenderer {
    /// Charge la bibliothèque pdfium.
    ///
    /// Cherche dans l'ordre (voir [`Self::search_dirs`]) puis se rabat sur
    /// les chemins système (`PATH`, `LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`).
    pub fn load() -> Result<Self> {
        let pdfium = shared_pdfium()?;
        Ok(Self {
            pdfium,
            parsed: RefCell::new(None),
            #[cfg(test)]
            parses: Cell::new(0),
        })
    }

    /// Exécute `with` sur le document pdfium de `doc`, reparsé seulement si
    /// la révision a changé.
    ///
    /// `PdfDocument` emprunte la bibliothèque ; le renderer ne peut donc pas
    /// exposer une référence empruntée depuis `&self` (elle vivrait dans un
    /// `RefCell`). Le rendu se fait dans la fermeture, tant que le parseur
    /// est accessible.
    fn with_parsed<T>(
        &self,
        doc: &Document,
        with: impl FnOnce(&PdfDocument<'_>) -> Result<T>,
    ) -> Result<T> {
        let key = (doc.instance_id(), doc.generation());

        // Sérialisation hors du verrou pdfium : elle ne touche pas à la
        // bibliothèque et peut être longue sur un gros document. Le cache
        // appartient à ce renderer seul, il ne peut pas changer entre-temps.
        let needs_parse = self.parsed.borrow().as_ref().map(|(k, _)| *k) != Some(key);
        let bytes = if needs_parse {
            Some(doc.snapshot()?.as_ref().clone())
        } else {
            None
        };

        let _lock = pdfium_lock();
        let mut parsed = self.parsed.borrow_mut();
        if let Some(bytes) = bytes {
            // `load_pdf_from_byte_vec` prend possession des octets : pdfium
            // peut alors lire le document sans copie intermédiaire.
            let pdf = self
                .pdfium
                .load_pdf_from_byte_vec(bytes, None)
                .map_err(|e| Error::Pdf(format!("chargement pdfium : {e}")))?;
            *parsed = Some((key, pdf));
            #[cfg(test)]
            self.parses.set(self.parses.get() + 1);
        }
        let (_, pdf) = parsed.as_ref().expect("parseur pdfium en cache");
        with(pdf)
    }

    /// Page `page_number` (1-indexée) du document parsé.
    fn page<'a>(pdf: &'a PdfDocument<'_>, page_number: u32) -> Result<PdfPage<'a>> {
        let pages = pdf.pages();
        // pdfium indexe les pages sur 16 bits : au-delà, la conversion
        // tronquerait et rendrait la mauvaise page.
        let index = u16::try_from(page_number.saturating_sub(1)).map_err(|_| {
            Error::InvalidInput(format!("numéro de page hors limites : {page_number}"))
        })?;
        pages
            .get(index)
            .map_err(|e| Error::Pdf(format!("page {page_number} introuvable : {e}")))
    }

    /// Répertoires candidats, dans l'ordre de priorité.
    ///
    /// 1. `$YOUPDF_PDFIUM_DIR` (surcharge explicite).
    /// 2. `$PDFIUM_DIR`.
    /// 3. À côté de l'exécutable : `./pdfium/` puis `./`.
    /// 4. En remontant depuis l'exécutable (cas `cargo run` : le binaire est
    ///    dans `target/{debug,release}/` alors que `pdfium/` est à la racine
    ///    du dépôt).
    /// 5. Depuis le répertoire courant.
    fn search_dirs() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();

        for var in ["YOUPDF_PDFIUM_DIR", "PDFIUM_DIR"] {
            if let Some(p) = std::env::var_os(var).filter(|v| !v.is_empty()) {
                dirs.push(PathBuf::from(p));
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                dirs.push(dir.join("pdfium"));
                dirs.push(dir.to_path_buf());
                // `target/release` -> `target` -> racine du dépôt.
                for anc in dir.ancestors().take(5) {
                    dirs.push(anc.join("pdfium"));
                }
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd.join("pdfium"));
            dirs.push(cwd.clone());
        }

        dirs
    }

    /// Chemin complet de la bibliothèque, si l'un des répertoires candidats
    /// la contient.
    fn locate_library() -> Option<PathBuf> {
        let name = Pdfium::pdfium_platform_library_name_at_path(".");
        let name = Path::new(&name)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "libpdfium".to_string());

        Self::search_dirs().into_iter().find_map(|dir| {
            let p = dir.join(&name);
            p.is_file().then_some(p)
        })
    }

    fn bind_pdfium() -> Result<Box<dyn PdfiumLibraryBindings>> {
        let mut failures: Vec<String> = Vec::new();

        // 1. Bibliothèque embarquée (répertoires candidats).
        if let Some(path) = Self::locate_library() {
            match Pdfium::bind_to_library(&path) {
                Ok(b) => {
                    tracing::info!(path = %path.display(), "bibliothèque pdfium localisée");
                    return Ok(b);
                }
                Err(e) => failures.push(format!("{} : {e}", path.display())),
            }
        }

        // 2. Bibliothèque système.
        match Pdfium::bind_to_system_library() {
            Ok(b) => return Ok(b),
            Err(e) => failures.push(format!("bibliothèque système : {e}")),
        }

        // Message d'aide en dur (les motifs d'artifact contiennent des
        // accolades, inoffensives ici car pas de `format!` sur ce bloc).
        const HELP: &str = "\
\nTéléchargez-la depuis https://github.com/bblanchon/pdfium-binaries (par ex.\
 `pdfium-mac-arm64.zip`, `pdfium-mac-x64.zip`, `pdfium-linux-x64.tgz`),\
 décompressez-la dans `./pdfium/` à la racine du dépôt (ou à côté du binaire),\
 ou pointez YOUPDF_PDFIUM_DIR vers son répertoire.\
\nSur macOS, une bibliothèque téléchargée porte l'attribut « quarantine » et le\
 chargement est refusé (« library load disallowed by system policy »). Réparez :\
\n  xattr -d com.apple.quarantine pdfium/libpdfium.dylib \\
\n    && codesign --force --sign - pdfium/libpdfium.dylib\
\n(`./scripts/install-pdfium.sh` fait tout cela).";

        let mut msg = format!(
            "bibliothèque pdfium introuvable ou chargeable :\n  {}",
            failures.join("\n  ")
        );
        msg.push_str(HELP);

        // Cas fréquent : fichier bien présent mais refusé par macOS.
        if let Some(path) = Self::locate_library() {
            let prefix = path.display().to_string();
            if failures.iter().any(|f| f.starts_with(&prefix)) {
                msg.push_str(&format!(
                    "\n  Fichier localisé mais refusé : {prefix} — quarantine macOS ou \
                     signature invalide."
                ));
            }
        }
        Err(Error::Config(msg))
    }

    /// Fragments de texte extraits par pdfium.
    ///
    /// pdfium résout les polices CID via leur table Unicode : c'est la seule
    /// source correcte pour les documents qui écrivent le texte en chaînes
    /// hexadécimales, illisibles pour un simple lecteur de flux. Les
    /// rectangles sont en repère page (origine en bas à gauche), comme
    /// [`TextFragment::rect`].
    fn fragments_from_pdfium(&self, doc: &Document, page_number: u32) -> Result<Vec<TextFragment>> {
        self.with_parsed(doc, |pdf| {
            let page = Self::page(pdf, page_number)?;
            let text = page
                .text()
                .map_err(|e| Error::Pdf(format!("texte de la page {page_number} : {e}")))?;

            let mut fragments = Vec::new();
            for segment in text.segments().iter() {
                // pdfium peut fusionner deux lignes visuelles dans un même
                // segment : on garde le saut de ligne, normalisé.
                let content = segment.text().replace("\r\n", "\n").replace('\r', "\n");
                if content.trim().is_empty() {
                    continue;
                }
                let bounds = segment.bounds();
                // Taille de police : celle du premier caractère du segment. Le
                // rectangle sert de repli quand pdfium ne la donne pas.
                let font_size = segment
                    .chars()
                    .ok()
                    .and_then(|chars| chars.iter().next().map(|c| c.scaled_font_size().value))
                    .filter(|size| *size > 0.0)
                    .unwrap_or_else(|| bounds.height().value);
                // Boîtes réelles des glyphes : la sélection peut alors s'arrêter
                // au caractère au lieu de prendre le segment entier.
                let chars: Vec<TextChar> = segment
                    .chars()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                let ch = item.unicode_char()?;
                                if ch == '\r' || ch == '\n' {
                                    return None;
                                }
                                let box_ = item.tight_bounds().ok()?;
                                Some(TextChar {
                                    ch,
                                    rect: [
                                        box_.left().value,
                                        box_.bottom().value,
                                        box_.right().value,
                                        box_.top().value,
                                    ],
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                fragments.push(TextFragment {
                    page: page_number as usize,
                    chars,
                    text: content,
                    rect: [
                        bounds.left().value,
                        bounds.bottom().value,
                        bounds.right().value,
                        bounds.top().value,
                    ],
                    font: None,
                    font_size: font_size.max(1.0),
                    color: [0, 0, 0],
                });
            }
            Ok(fragments)
        })
    }
}

impl Renderer for PdfiumRenderer {
    fn render_page(
        &self,
        doc: &Document,
        page_number: u32,
        options: &RenderOptions,
    ) -> Result<PageBitmap> {
        self.with_parsed(doc, |pdf| {
            let page = Self::page(pdf, page_number)?;

            let config = PdfRenderConfig::new()
                .set_target_width((page.width().value * options.scale) as i32)
                .set_maximum_height((page.height().value * options.scale) as i32)
                .render_form_data(true)
                .render_annotations(options.render_annotations)
                .use_lcd_text_rendering(options.antialias);

            let bitmap = page
                .render_with_config(&config)
                .map_err(|e| Error::Pdf(format!("rendu de la page {page_number} : {e}")))?;

            let width = bitmap.width() as u32;
            let height = bitmap.height() as u32;
            let pixels = bitmap.as_rgba_bytes();

            Ok(PageBitmap {
                width,
                height,
                pixels,
            })
        })
    }

    fn page_fragments(&self, doc: &Document, page_number: u32) -> Result<Vec<TextFragment>> {
        // pdfium a la priorité : il décode les polices CID. S'il ne trouve
        // rien (page vierge, flux qu'il ignore) ou échoue, le décodeur
        // interne sert de filet plutôt que de renvoyer une page vide.
        match self.fragments_from_pdfium(doc, page_number) {
            Ok(fragments) if !fragments.is_empty() => Ok(fragments),
            _ => doc.extract_text_fragments(page_number),
        }
    }
}

impl Drop for PdfiumRenderer {
    fn drop(&mut self) {
        // Libérer le document parsé appelle `FPDF_CloseDocument` : c'est un
        // appel pdfium comme un autre, à sérialiser avec les rendus des
        // autres renderers.
        if self.parsed.borrow().is_some() {
            let _lock = pdfium_lock();
            self.parsed.borrow_mut().take();
        }
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

#[cfg(test)]
mod tests {
    #[test]
    fn background_color_around_takes_median_of_border_samples() {
        // Trois bandes verticales : noir (x < 42), gris 128 (42..58), blanc.
        // Un bloc centre echantillonne x = 38, 50, 62 sur deux rangees :
        // la mediane par canal est le gris du milieu.
        let w = 100;
        let h = 100;
        let mut pixels = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = if x < 42 { 0 } else if x < 58 { 128 } else { 255 };
                let i = (y * w + x) * 4;
                pixels[i] = v;
                pixels[i + 1] = v;
                pixels[i + 2] = v;
                pixels[i + 3] = 255;
            }
        }
        let bitmap = PageBitmap {
            width: w as u32,
            height: h as u32,
            pixels,
        };
        // rect en repere affiche (y vers le haut), scale = 1 pt = 1 px.
        let color = bitmap
            .background_color_around(1.0, [30.0, 40.0, 70.0, 60.0])
            .unwrap();
        assert_eq!(color, [128, 128, 128], "mediane attendue au milieu : {color:?}");
        // Aucun point utilisable hors bitmap -> None.
        assert!(bitmap
            .background_color_around(1.0, [500.0, 500.0, 600.0, 600.0])
            .is_none());
        assert!(bitmap.background_color_around(0.0, [0.0, 0.0, 1.0, 1.0]).is_none());
    }

    use super::*;
    use crate::{Annotation, AnnotationKind};

    /// Ancienne conversion, conservée comme référence de non-régression :
    /// elle construisait un tampon RGBA octet par octet puis le faisait
    /// relire par `ColorImage::from_rgba_unmultiplied`.
    fn ancienne_conversion(bitmap: &PageBitmap) -> egui::ColorImage {
        let expected = bitmap.width as usize * bitmap.height as usize * 4;
        if bitmap.pixels.len() < expected {
            return egui::ColorImage::new(
                [bitmap.width as usize, bitmap.height as usize],
                egui::Color32::from_rgb(240, 240, 240),
            );
        }
        let mut rgba = Vec::with_capacity(expected);
        for chunk in bitmap.pixels[..expected].chunks_exact(4) {
            rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
        egui::ColorImage::from_rgba_unmultiplied(
            [bitmap.width as usize, bitmap.height as usize],
            &rgba,
        )
    }

    /// Les pixels produits doivent rester identiques à ceux de l'ancienne
    /// conversion, octet pour octet, quel que soit l'alpha de la source.
    #[test]
    fn conversion_identique_octet_pour_octet() {
        let entrees = [
            // Tampon « BGRx » de pdfium : le quatrième octet est un
            // remplissage, pas un alpha — il ne doit pas ressortir tel quel.
            PageBitmap {
                width: 2,
                height: 2,
                pixels: vec![
                    0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x00, 0x12, 0xFF, 0x00, 0x00, 0xFF,
                    0x10, 0x20, 0x30, 0x40,
                ],
            },
            // Tampon déjà RGBA (`PageBitmap::solid`) : un rouge pur doit
            // rester rouge.
            PageBitmap::solid(3, 1, [0xFF, 0x00, 0x00, 0xFF]),
            // Six pixels quelconques, alpha variable compris.
            PageBitmap {
                width: 3,
                height: 2,
                pixels: vec![
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                    22, 23, 24,
                ],
            },
            // Tampon trop court : repli sur le gris uniforme.
            PageBitmap {
                width: 2,
                height: 2,
                pixels: vec![1, 2, 3, 4, 5, 6],
            },
            // Bitmap vide.
            PageBitmap {
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
        ];
        for bitmap in entrees {
            let attendu = ancienne_conversion(&bitmap);
            let obtenu = bitmap.to_egui_image();
            assert_eq!(obtenu.size, attendu.size);
            assert_eq!(
                obtenu.pixels, attendu.pixels,
                "conversion divergente pour {}x{}",
                bitmap.width, bitmap.height
            );
        }
    }

    /// L'alpha de la source n'est pas recopié : la page reste opaque.
    #[test]
    fn alpha_force_a_255() {
        let bitmap = PageBitmap {
            width: 1,
            height: 1,
            pixels: vec![0x10, 0x20, 0x30, 0x00],
        };
        let image = bitmap.to_egui_image();
        assert_eq!(image.pixels[0].to_array(), [0x10, 0x20, 0x30, 0xFF]);
    }

    /// Les canaux ne sont pas permutés : un rouge pur ressort rouge.
    #[test]
    fn canaux_preserves() {
        let image = PageBitmap::solid(1, 1, [0xFF, 0x00, 0x00, 0xFF]).to_egui_image();
        assert_eq!(image.pixels[0].to_array(), [0xFF, 0x00, 0x00, 0xFF]);
    }

    /// Page 612×792 portant un texte, pour que le rendu ne soit pas vide.
    fn page_avec_texte() -> Vec<u8> {
        use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

        let mut doc = LopdfDoc::with_version("1.7");
        let font = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        });
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 60 700 Td (YouPDF) Tj ET".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        let pages = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![page.into()]),
            "Count" => 1,
        });
        let catalog = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => pages,
        });
        doc.trailer.set("Root", Object::Reference(catalog));
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("écriture du PDF de test");
        out
    }

    /// Pixels franchement jaunes : la trace du surlignage.
    fn pixels_jaunes(bitmap: &PageBitmap) -> usize {
        bitmap
            .pixels
            .chunks_exact(4)
            .filter(|px| px[0] as i32 - px[2] as i32 > 40 && px[1] as i32 - px[2] as i32 > 30)
            .count()
    }

    /// Renderer pdfium, ou `None` si la bibliothèque manque (test ignoré).
    fn renderer() -> Option<PdfiumRenderer> {
        match PdfiumRenderer::load() {
            Ok(renderer) => Some(renderer),
            Err(e) => {
                eprintln!("pdfium indisponible : test ignoré ({e})");
                None
            }
        }
    }

    /// Le document ne doit être parsé qu'une fois par révision : rendu et
    /// extraction de texte successifs réutilisent le même `PdfDocument`.
    #[test]
    fn le_parseur_pdfium_est_reutilise() {
        let Some(renderer) = renderer() else {
            return;
        };
        let doc = Document::from_bytes(&page_avec_texte()).expect("ouverture");
        let options = RenderOptions::default();

        renderer
            .render_page(&doc, 1, &options)
            .expect("premier rendu");
        assert_eq!(renderer.parses.get(), 1, "le premier rendu doit parser");

        renderer
            .render_page(&doc, 1, &options)
            .expect("second rendu");
        renderer.page_fragments(&doc, 1).expect("fragments");
        assert_eq!(
            renderer.parses.get(),
            1,
            "le document a été reparsé à révision constante"
        );
    }

    /// Une modification du document doit invalider le parseur en cache : le
    /// rendu suivant doit montrer le nouveau contenu, sous peine d'afficher
    /// l'ancien document à l'écran.
    #[test]
    fn une_modification_invalide_le_parseur() {
        let Some(renderer) = renderer() else {
            return;
        };
        let mut doc = Document::from_bytes(&page_avec_texte()).expect("ouverture");
        let options = RenderOptions::default();

        let avant = renderer
            .render_page(&doc, 1, &options)
            .expect("rendu avant");
        assert_eq!(pixels_jaunes(&avant), 0, "page déjà marquée ?");
        assert_eq!(renderer.parses.get(), 1);

        let generation = doc.generation();
        doc.apply_annotations(&[Annotation::new(
            1,
            AnnotationKind::Highlight,
            [100.0, 400.0, 400.0, 440.0],
        )])
        .expect("annotation");
        assert!(
            doc.generation() > generation,
            "une mutation doit incrémenter la génération"
        );

        let apres = renderer
            .render_page(&doc, 1, &options)
            .expect("rendu après");
        assert_eq!(
            renderer.parses.get(),
            2,
            "le document modifié doit être reparsé"
        );
        assert!(
            pixels_jaunes(&apres) > 0,
            "le rendu ne reflète pas l'annotation ajoutée : parseur périmé"
        );
    }
}
