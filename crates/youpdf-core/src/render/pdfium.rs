//! Implémentation pdfium du rendu.
//!
//! pdfium n'est pas thread-safe : les appels sont sérialisés par un
//! mutex global, et le document parsé est mis en cache par renderer.

use std::cell::RefCell;
#[cfg(test)]
use std::cell::Cell;
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

use youpdf_shared::{Error, Result};



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
pub(crate) fn pdfium_lock() -> MutexGuard<'static, ()> {
    PDFIUM_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Charge (une seule fois) la bibliothèque pdfium du processus.
pub(crate) fn shared_pdfium() -> Result<&'static Pdfium> {
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
    pub(super) pdfium: &'static Pdfium,
    pub(super) parsed: RefCell<Option<((u64, u64), PdfDocument<'static>)>>,
    /// Nombre de parsings effectués (tests : le cache doit servir).
    #[cfg(test)]
    pub(super) parses: Cell<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{PageBitmap, RenderOptions, Renderer};

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
    use crate::{Annotation, AnnotationKind, Document};

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
