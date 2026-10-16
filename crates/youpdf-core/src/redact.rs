//! Caviardage (redaction) destructif.
//!
//! ## Principe
//!
//! Le caviardage PDF est **destructif par nature** : il doit être
//! impossible de récupérer le texte sous le rectangle noir. Un simple
//! rectangle dessiné par-dessus ne suffit pas — n'importe qui peut
//! extraire le texte sous-jacent.
//!
//! ## Approche retenue
//!
//! YouPDF rasterise **la page entière** :
//!
//! 1. Rendu de la page en bitmap haute résolution via pdfium.
//! 2. Remplissage en noir de chaque région à caviarder.
//! 3. Remplacement du contenu de la page par cette image.
//!
//! Le texte original est irrécupérable. L'inconvénient est que **toute
//! la page devient une image** : le texte non caviardé n'est plus
//! sélectionnable. Pour compenser, une couche texte OCR peut être
//! régénérée sur les zones non caviardées (phase 5).
//!
//! Les **annotations**, elles, ne sont pas rasterisées : elles restent
//! des objets vectoriels vivants à côté de l'image (voir
//! [`Document::classify_annots_for_redaction`]). Une annotation qui
//! recouvre une zone caviardée est **détruite** — c'est le comportement
//! attendu : un tampon, un surlignage ou un champ de formulaire peut
//! masquer un mot sensible sans le contenir dans le flux de contenu, et
//! l'incruster dans l'image pour le « conserver » reviendrait à
//! reconstruire exactement ce qu'on vient d'effacer.
//!
//! ## Alternatives écartées
//!
//! - Suppression ciblée des instructions de texte dans le flux de
//!   contenu : très fragile, ne gère pas les polices subset, les
//!   opérateurs composés, les XObjects imbriqués.
//! - Rectangle noir en annotation : non destructif, trompeur.

use lopdf::{dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::render::{RenderOptions, Renderer};

/// Une région à caviarder sur une page.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionRegion {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle en coordonnées PDF : `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
}

impl Document {
    /// Applique un caviardage destructif.
    ///
    /// `regions` liste toutes les zones à noircir sur toutes les pages.
    /// Les pages sans région restent intactes.
    ///
    /// `renderer` fournit le rendu haute résolution (typiquement
    /// `PdfiumRenderer`).
    pub fn redact(&mut self, regions: &[RedactionRegion], renderer: &dyn Renderer) -> Result<()> {
        if regions.is_empty() {
            return Ok(());
        }

        // Le caviardage **remplace** la page par une image : si le renderer
        // ne dessine rien (`NullRenderer`), cette opération détruit le texte
        // d'origine et produit des pages vides… tout en renvoyant `Ok(())`.
        // C'est le pire résultat possible pour un outil de confidentialité.
        if !renderer.renders_real_content() {
            return Err(Error::NotSupported(
                "caviardage impossible sans rendu réel : la bibliothèque pdfium est \
                 indisponible, et l'opération remplacerait les pages par des images vides \
                 en supprimant le texte d'origine. Installez pdfium (voir pdfium/README.md) \
                 puis relancez l'opération."
                    .into(),
            ));
        }

        // Grouper par page.
        let mut by_page: std::collections::HashMap<usize, Vec<&RedactionRegion>> =
            std::collections::HashMap::new();
        for r in regions {
            by_page.entry(r.page).or_default().push(r);
        }

        // Résolution élevée pour que le texte reste lisible autour.
        // 2.5 ≈ 180 DPI, bon compromis taille / qualité.
        const SCALE: f32 = 2.5;

        for (page_num, page_regions) in by_page {
            let page_id = self
                .page_id(page_num as u32)
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;

            // 1. Rendu **sans** les annotations : on les garde en
            // vectoriel, on ne les incruste pas dans l'image.
            let opts = RenderOptions {
                scale: SCALE,
                ..Default::default()
            };
            let bitmap = renderer.render_page(self, page_num as u32, &opts)?;

            // 2. Noir sur les régions.
            let (w_pt, h_pt) = self.page_size(page_num as u32);
            let mut pixels = bitmap.pixels.clone();
            for region in &page_regions {
                paint_black_rect(
                    &mut pixels,
                    bitmap.width,
                    bitmap.height,
                    w_pt,
                    h_pt,
                    SCALE,
                    region.rect,
                );
            }

            // 3. Tri des annotations : celles qui touchent une zone
            // caviardée sont détruites, les autres survivent.
            let (kept, dropped) = self.classify_annots_for_redaction(page_id, &page_regions)?;

            // 4. Remplacer le contenu de la page par l'image.
            self.replace_page_with_image(
                page_id,
                &pixels,
                bitmap.width,
                bitmap.height,
                &kept,
            )?;

            if dropped > 0 {
                tracing::warn!(
                    page = page_num,
                    annotations_supprimees = dropped,
                    "des annotations couvraient une zone caviardée : elles ont été                      supprimées (elles pouvaient masquer le contenu sensible)"
                );
            }
            tracing::info!(
                page = page_num,
                regions = page_regions.len(),
                annotations_conservees = kept.len(),
                "caviardage appliqué"
            );
        }

        Ok(())
    }

    /// Trie les annotations de la page avant caviardage.
    ///
    /// Renvoie `(à conserver, nombre de détruites)`.
    ///
    /// Une annotation est détruite dès que sa zone `/Rect` recouvre une
    /// région caviardée. Ce n'est pas un dommage collatéral, c'est le
    /// comportement **correct** : un surlignage, un tampon texte ou un
    /// champ de formulaire peut porter le mot sensible lui-même, ou le
    /// masquer. Les conserver (à plus forte raison les incruster dans
    /// l'image) laisserait fuir précisément ce qu'on venait d'effacer.
    ///
    /// Les `/<Popup>` d'une annotation détruite suivent la même
    /// règle : les laisser produirait un encadré flottant orphelin.
    ///
    /// Les annotations sans `/Rect` exploitable (celles héritées d'un
    /// nœud parent de l'arbre des pages, cas rarissime) sont détruites
    /// par précaution — on ne peut pas prouver qu'elles sont hors zone.
    pub(crate) fn classify_annots_for_redaction(
        &self,
        page_id: lopdf::ObjectId,
        regions: &[&RedactionRegion],
    ) -> Result<(Vec<lopdf::ObjectId>, usize)> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let annots = match page.get(b"Annots") {
            Ok(Object::Array(items)) => items.clone(),
            // Pas d'annotations : rien à faire.
            Ok(Object::Reference(id)) => match self.inner().get_object(*id) {
                Ok(Object::Array(items)) => items.clone(),
                _ => return Ok((Vec::new(), 0)),
            },
            _ => return Ok((Vec::new(), 0)),
        };

        // Résolution des références en identifiants directs.
        let mut ids: Vec<lopdf::ObjectId> = Vec::with_capacity(annots.len());
        for item in &annots {
            match item {
                Object::Reference(id) => ids.push(*id),
                // Annotation en ligne (dictionnaire direct) : elle est
                // intégrée au tableau, donc solidaire de la page → détruite.
                Object::Dictionary(_) => {}
                _ => {}
            }
        }
        let inline_count = annots.len() - ids.len();

        let mut doomed: Vec<lopdf::ObjectId> = Vec::new();
        let mut kept: Vec<lopdf::ObjectId> = Vec::new();
        for id in ids {
            let overlaps = match self.annot_rect(id) {
                Some(rect) => regions.iter().any(|r| rects_overlap(rect, r.rect)),
                // Sans zone connue, impossible de garantir l'innocuité.
                None => true,
            };
            if overlaps {
                doomed.push(id);
                // Le volet de commentaires attaché tombe avec elle.
                if let Some(popup) = self.annot_popup(id) {
                    doomed.push(popup);
                }
            } else {
                kept.push(id);
            }
        }
        kept.retain(|id| !doomed.contains(id));

        Ok((kept, doomed.len() + inline_count))
    }

    /// `/Rect` d'une annotation, en coordonnées de page.
    fn annot_rect(&self, id: lopdf::ObjectId) -> Option<[f32; 4]> {
        let dict = self.inner().get_object(id).ok()?.as_dict().ok()?;
        let arr = dict.get(b"Rect").ok()?.as_array().ok()?;
        if arr.len() < 4 {
            return None;
        }
        let n = |i: usize| {
            arr[i]
                .as_float()
                .or_else(|_| arr[i].as_i64().map(|v| v as f32))
                .ok()
        };
        let (a, b, c, d) = (n(0)?, n(1)?, n(2)?, n(3)?);
        Some([a.min(c), b.min(d), a.max(c), b.max(d)])
    }

    /// Référence de l'objet `/<Popup>` d'une annotation, si elle en a un.
    fn annot_popup(&self, id: lopdf::ObjectId) -> Option<lopdf::ObjectId> {
        let dict = self.inner().get_object(id).ok()?.as_dict().ok()?;
        dict.get(b"Popup").ok()?.as_reference().ok()
    }

    /// Remplace le contenu d'une page par une image XObject unique.
    ///
    /// `kept_annots` liste les annotations à conserver telles quelles
    /// (cf. [`Document::classify_annots_for_redaction`]) : leurs objets
    /// propres sont réinjectés dans les ressources de la page, puisque
    /// l'ancienne dictionnaire de ressources est écrasée.
    fn replace_page_with_image(
        &mut self,
        page_id: (u32, u16),
        rgba: &[u8],
        width: u32,
        height: u32,
        kept_annots: &[lopdf::ObjectId],
    ) -> Result<()> {
        // Dimensions de la page en points.
        let page_dict = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let (w_pt, h_pt) = read_mediabox(page_dict).unwrap_or((595.0, 842.0));

        // Conversion RGBA → RGB.
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        for chunk in rgba.chunks_exact(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }

        // Compression JPEG : une page A4 rastérisée en RGB brut pèse
        // ~25 Mo, en JPEG ~200 Ko. La qualité 88 reste lisible à l'écran.
        let mut jpeg: Vec<u8> = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88);
        encoder
            .encode(
                &rgb,
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| Error::Pdf(format!("encodage JPEG de la page : {e}")))?;

        // Image XObject.
        let image_dict = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
            "Filter" => Object::Name(b"DCTDecode".to_vec()),
        };
        let image_stream = Stream::new(image_dict, jpeg);
        let image_id = self.inner_mut().add_object(image_stream);

        // Flux de contenu : dessine l'image pour couvrir toute la page.
        let content = format!("q\n{} 0 0 {} 0 0 cm\n/Im0 Do\nQ\n", w_pt, h_pt);
        let content_stream = Stream::new(dictionary! {}, content.into_bytes());
        let content_id = self.inner_mut().add_object(content_stream);

        // Ressources de la page : on écrase par un XObject propre.
        let mut resources = dictionary! {
            "XObject" => dictionary! {
                "Im0" => Object::Reference(image_id),
            },
        };

        // Une couche d'apparence peut hériter des ressources de la page
        // (son dictionnaire `/Resources` est facultatif). Comme on
        // écrase ces ressources, on y reporte celles de l'ancienne :
        // sinon un surlignage conservé peut se retrouver sans police ni
        // état graphique, et disparaître à l'écran.
        if !kept_annots.is_empty() {
            if let Ok(old_res) = self.inner().get_page_resources(page_id) {
                if let Some(prev) = old_res.0 {
                    for key in [b"ExtGState".as_slice(), b"Font".as_slice(), b"XObject".as_slice()]
                    {
                        if let Ok(value) = prev.get(key) {
                            let value = value.clone();
                            if key == b"XObject" {
                                // Le nom /Im0 est déjà pris par notre image.
                                if let Object::Dictionary(mut dicts) = value {
                                    dicts.remove(b"Im0");
                                    resources.set("XObject", Object::Dictionary(dicts));
                                }
                            } else {
                                resources.set(key, value);
                            }
                        }
                    }
                }
            }
        }

        // Mise à jour de la page.
        let page_obj = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page_obj
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        // On remplace entièrement /Contents et /Resources.
        dict.set("Contents", Object::Reference(content_id));
        dict.set("Resources", Object::Dictionary(resources));

        // Et l'on ne garde que les annotations jugées sures.
        if kept_annots.is_empty() {
            dict.remove(b"Annots");
        } else {
            dict.set(
                "Annots",
                Object::Array(kept_annots.iter().map(|id| Object::Reference(*id)).collect()),
            );
        }

        Ok(())
    }
}

/// Vrai si deux rectangles PDF se recouvrent (aire strictement positive).
///
/// Un simple contact de bord ne compte pas : une annotation posée
/// exactement à cheval sur la limite d'une zone caviardée ne cache rien.
fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    let inter_w = a[2].min(b[2]) - a[0].max(b[0]);
    let inter_h = a[3].min(b[3]) - a[1].max(b[1]);
    inter_w > 0.0 && inter_h > 0.0
}

/// Lit le MediaBox d'une page.
fn read_mediabox(dict: &lopdf::Dictionary) -> Option<(f32, f32)> {
    let arr = dict.get(b"MediaBox").ok()?.as_array().ok()?;
    if arr.len() < 4 {
        return None;
    }
    let num = |i: usize| -> f32 {
        arr[i]
            .as_float()
            .or_else(|_| arr[i].as_i64().map(|v| v as f32))
            .unwrap_or(0.0)
    };
    let w = (num(2) - num(0)).abs();
    let h = (num(3) - num(1)).abs();
    if w > 0.0 && h > 0.0 {
        Some((w, h))
    } else {
        None
    }
}

/// Remplit un rectangle en noir dans un bitmap RGBA.
///
/// Le rectangle est fourni en coordonnées PDF (origine bas-gauche).
/// Le bitmap est en coordonnées écran (origine haut-gauche).
fn paint_black_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    page_w_pt: f32,
    page_h_pt: f32,
    scale: f32,
    rect: [f32; 4],
) {
    let [x1, y1, x2, y2] = rect;
    let (rx1, rx2) = (x1.min(x2), x1.max(x2));
    let (ry1, ry2) = (y1.min(y2), y1.max(y2));

    // Conversion PDF → pixel.
    let px1 = (rx1 * scale).clamp(0.0, width as f32) as u32;
    let px2 = (rx2 * scale).clamp(0.0, width as f32) as u32;
    // L'axe y est inversé : PDF bas-gauche, pixels haut-gauche.
    let py1 = ((page_h_pt - ry2) * scale).clamp(0.0, height as f32) as u32;
    let py2 = ((page_h_pt - ry1) * scale).clamp(0.0, height as f32) as u32;

    let (py1, py2) = (py1.min(py2), py1.max(py2));

    for y in py1..py2 {
        for x in px1..px2 {
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < pixels.len() {
                pixels[idx] = 0; // R
                pixels[idx + 1] = 0; // G
                pixels[idx + 2] = 0; // B
                pixels[idx + 3] = 255; // A
            }
        }
    }

    // Note : on ne touche pas à `page_w_pt`, mais on le garde dans la
    // signature pour d'éventuelles vérifications futures.
    let _ = page_w_pt;
    let _ = Error::Pdf; // évite un avertissement d'import inutilisé
}
