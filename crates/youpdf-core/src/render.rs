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

use std::cell::RefCell;
use std::path::{Path, PathBuf};

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

    /// Construit un `egui::ColorImage` à partir du bitmap.
    ///
    /// Conversion directe : pdfium produit du BGRA, egui attend du RGBA.
    pub fn to_egui_image(&self) -> egui::ColorImage {
        let expected = self.width as usize * self.height as usize * 4;
        if self.pixels.len() < expected {
            return egui::ColorImage::new(
                [self.width as usize, self.height as usize],
                egui::Color32::from_rgb(240, 240, 240),
            );
        }
        // `PdfBitmap::as_rgba_bytes` normalise DÉJÀ le tampon en RGBA (il
        // retourne le BGRx natif de pdfium dans l'ordre RGBA). On recopiait
        // R et B l'un dans l'autre : un rouge pur (#FF0000) sortait en bleu
        // (#0000FF) à l'écran, et les tampons `PageBitmap::solid` — déjà
        // RGBA — étaient eux aussi inversés.
        let mut rgba = Vec::with_capacity(expected);
        for chunk in self.pixels[..expected].chunks_exact(4) {
            rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &rgba,
        )
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

/// Implémentation pdfium.
///
/// pdfium n'est **pas** thread-safe : le handle est protégé par un
/// `RefCell` et `PdfiumRenderer` n'est donc **pas** `Sync`. On l'utilise
/// uniquement depuis le thread d'interface (egui est mono-thread côté rendu).
pub struct PdfiumRenderer {
    pdfium: RefCell<Pdfium>,
}

impl PdfiumRenderer {
    /// Charge la bibliothèque pdfium.
    ///
    /// Cherche dans l'ordre (voir [`Self::search_dirs`]) puis se rabat sur
    /// les chemins système (`PATH`, `LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`).
    pub fn load() -> Result<Self> {
        let bindings = Self::bind_pdfium()?;
        tracing::info!("pdfium chargé avec succès");
        Ok(Self {
            pdfium: RefCell::new(Pdfium::new(bindings)),
        })
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
    fn fragments_from_pdfium(
        &self,
        doc: &Document,
        page_number: u32,
    ) -> Result<Vec<TextFragment>> {
        let bytes = doc.snapshot()?;
        let pdfium = self.pdfium.borrow();
        let pdf = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| Error::Pdf(format!("chargement pdfium : {e}")))?;

        let pages = pdf.pages();
        let index = u16::try_from(page_number.saturating_sub(1)).map_err(|_| {
            Error::InvalidInput(format!("numéro de page hors limites : {page_number}"))
        })?;
        let page = pages
            .get(index)
            .map_err(|e| Error::Pdf(format!("page {page_number} introuvable : {e}")))?;
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
            fragments.push(TextFragment {
                page: page_number as usize,
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
    }
}

impl Renderer for PdfiumRenderer {
    fn render_page(
        &self,
        doc: &Document,
        page_number: u32,
        options: &RenderOptions,
    ) -> Result<PageBitmap> {
        // Octets partagés via `Rc` : pas de re-sérialisation ni de copie
        // complète du document à chaque page.
        let bytes = doc.snapshot()?;
        let pdfium = self.pdfium.borrow();

        let pdf = pdfium
            .load_pdf_from_byte_slice(&bytes, None)
            .map_err(|e| Error::Pdf(format!("chargement pdfium : {e}")))?;

        let pages = pdf.pages();
        // pdfium indexe les pages sur 16 bits : au-delà, la conversion
        // tronquerait et rendrait la mauvaise page.
        let index = u16::try_from(page_number.saturating_sub(1)).map_err(|_| {
            Error::InvalidInput(format!("numéro de page hors limites : {page_number}"))
        })?;
        let page = pages
            .get(index)
            .map_err(|e| Error::Pdf(format!("page {page_number} introuvable : {e}")))?;

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
