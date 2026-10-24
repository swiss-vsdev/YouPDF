//! Méthodes de `PdfiumRenderer` : cache, rendu, extraction de texte.

use crate::document::Document;
use crate::edit::{TextChar, TextFragment};
use super::{PageBitmap, RenderOptions, Renderer};

use pdfium_render::prelude::*;

use youpdf_shared::{Error, Result};

use std::cell::RefCell;
#[cfg(test)]
use std::cell::Cell;
use std::path::{Path, PathBuf};

use super::pdfium::*;

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
    pub(crate) fn page<'a>(pdf: &'a PdfDocument<'_>, page_number: u32) -> Result<PdfPage<'a>> {
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

    pub(super) fn bind_pdfium() -> Result<Box<dyn PdfiumLibraryBindings>> {
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
    pub(crate) fn fragments_from_pdfium(&self, doc: &Document, page_number: u32) -> Result<Vec<TextFragment>> {
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
