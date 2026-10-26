//! Caviardage (redaction) destructif.
//!
//! ## Principe
//!
//! Le caviardage PDF est **destructif par nature** : il doit être
//! impossible de récupérer le texte sous le rectangle noir. Un simple
//! rectangle dessiné par-dessus ne suffit pas — n'importe qui peut
//! extraire le texte sous-jacent.
//!
//! ## Deux moteurs
//!
//! ### `Document::redact` — rasterisation intégrale
//!
//! Rendu de la page entière en bitmap, remplissage des zones, remplacement
//! du contenu par cette image. Le texte original est irrécupérable, mais
//! **toute la page devient une image** : le texte non caviardé n'est plus
//! sélectionnable. C'est le comportement historique, conservé tel quel.
//!
//! ### `Document::redact_marks` — chirurgie du flux de contenu
//!
//! Le moteur principal. Il réécrit le flux de contenu de la page :
//!
//! - les instructions de texte dont la boîte recouvre une zone caviardée
//!   sont **supprimées** du flux, et le reste de la page reste
//!   sélectionnable et cherchable ;
//! - les tracés vectoriels entièrement contenus dans la zone sont
//!   supprimés ;
//! - les images sont **brûlées** : les pixels sous la zone sont écrasés et
//!   l'image est réencodée, de sorte qu'aucun pixel d'origine ne
//!   subsiste ;
//! - les annotations qui recouvrent la zone sont **détruites** : un
//!   surlignage ou un champ peut porter le mot sensible lui-même.
//!
//! Une page dont le contenu ne peut pas être découpé proprement (image
//! inline, XObject de formulaire, tramé) est **rasterisée** : c'est le
//! repli sûr, au prix de la couche texte de cette page-là.
//!
//! ## Repères
//!
//! Les rectangles donnés par l'utilisateur sont dans le repère **affiché**
//! (rotation appliquée) ; le flux de contenu travaille dans le repère
//! **page**. La conversion se fait avec [`PageView::page_rect`].

use lopdf::{dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::{Document, PageView};
use crate::edit::PatchFont;
use crate::render::{RenderOptions, Renderer};

pub(crate) mod burn;
pub(crate) mod geometry;
pub(crate) mod surgery;

/// Une région à caviarder sur une page.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionRegion {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle `[x1, y1, x2, y2]` dans le repère **affiché** de la page :
    /// origine au coin inférieur gauche de la boîte visible, rotation
    /// appliquée. C'est le repère de ce que l'utilisateur voit, donc celui
    /// qu'il désigne à la souris.
    pub rect: [f32; 4],
}

/// Couleur de remplissage d'une zone caviardée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactionFill {
    /// Noir : le comportement par défaut d'un caviardage.
    #[default]
    Black,
    /// Blanc : pour masquer sans marquer la zone.
    White,
    /// Couleur personnalisée RGB.
    Custom([u8; 3]),
}

/// Texte superposé sur une zone caviardée, à la place du contenu effacé.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionOverlay {
    /// Texte à écrire, par exemple `[REDACTED]`.
    pub text: String,
    /// Police standard-14 utilisée.
    pub font: PatchFont,
    /// Corps en points.
    pub size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
}

/// Un rectangle à caviarder, avec son apparence.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionMark {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle `[x1, y1, x2, y2]` dans le repère **affiché**.
    pub rect: [f32; 4],
    /// Couleur de remplissage.
    pub fill: RedactionFill,
    /// Texte à superposer, si l'on souhaite remplacer le contenu effacé
    /// par une mention lisible.
    pub overlay: Option<RedactionOverlay>,
}

impl Default for RedactionMark {
    fn default() -> Self {
        Self {
            page: 1,
            rect: [0.0; 4],
            fill: RedactionFill::default(),
            overlay: None,
        }
    }
}

/// Bilan d'un caviardage : ce qui a été découpé, ce qui a dû être rasterisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RedactionReport {
    /// Pages dont le flux de contenu a été réécrit.
    pub pages_sliced: usize,
    /// Pages rasterisées en repli.
    pub pages_rasterized: usize,
    /// Annotations détruites.
    pub annotations_removed: usize,
    /// Instructions de texte supprimées du flux.
    pub text_ops_removed: usize,
    /// Tracés vectoriels supprimés du flux.
    pub vector_ops_removed: usize,
    /// Images dont les pixels ont été brûlés.
    pub images_burned: usize,
}

/// Options de traduction bilingale, en place, sans réflow.
///
/// Le principe : on remplace le texte original par la traduction en
/// **réduisant le corps** jusqu'à ce que la traduction tienne dans la
/// boîte d'origine. Aucun moteur de mise en page : c'est le point de
/// départ, et la dégradation est marquée par un commentaire
/// `ponytail` dans [`Document::translate_pages`].
#[derive(Debug, Clone, Default)]
pub struct TranslationOptions {
    /// Langue cible, telle que l'utilisateur la saisit.
    pub target_language: String,
    /// Vrai : original + traduction empilés dans la même boîte.
    pub bilingual: bool,
    /// Vrai : réduire le corps pour faire tenir le texte (pas de réflow).
    pub preserve_layout: bool,
}

/// Bilan d'une traduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TranslationReport {
    /// Pages dont le texte a été remplacé.
    pub pages_translated: usize,
    /// Pages qui ont dû être rasterisées en repli.
    pub pages_rasterized: usize,
    /// Pages sautées (sans texte, ou sans boîte exploitable).
    pub pages_skipped: usize,
    /// Lignes posées sur le document.
    pub lines_placed: usize,
}

/// Zebra les lignes originales et les lignes traduites par index.
///
/// ponytail: zip par position, pas par correspondance de contenu. Une
/// traduction qui compte un nombre de lignes différent garde les N
/// premières lignes originales alignées sur les N premières lignes
/// traduites ; les lignes excédentes de l'un ou l'autre côté sont
/// ignorées. Un vrai alignement nécessiterait des boîtes par fragment.
pub fn zip_lines<'a>(original: &'a [String], translated: &'a [String]) -> Vec<(&'a String, &'a String)> {
    original.iter().zip(translated.iter()).collect()
}

/// Taille de police qui fait tenir `lines` dans la boîte `box_w × box_h`,
/// en partant de `start` et en baisse par paliers de 0.5 pt.
///
/// Le plancher est 4.0 pt : en dessous, le texte est illisible et
/// l'opération n'a plus de sens.
///
/// ponytail: un seul colonne, modèle de largeur proportionnelles
/// (`average_width × size × nb_caractères`). Aucun word-wrap : un mot long
/// peut déborder la colonne même au plancher. Ajouter un vrai moteur de
/// mise en page si du justifié ou de la césure est un jour nécessaire.
pub fn fit_block_from(
    lines: &[String],
    box_w: f32,
    box_h: f32,
    line_spacing: f32,
    start: f32,
) -> f32 {
    if lines.is_empty() || box_w <= 0.0 || box_h <= 0.0 {
        return 4.0;
    }
    let fits = |s: f32| {
        if s < 4.0 {
            return false;
        }
        let row = s * line_spacing;
        if (lines.len() as f32) * row > box_h {
            return false;
        }
        lines
            .iter()
            .all(|l| geometry::estimate_width(l, s, "Helvetica") <= box_w)
    };
    let mut size = start.max(4.0);
    while !fits(size) && size > 4.0 {
        size -= 0.5;
    }
    size
}

/// Comme [`fit_block_from`], en partant de 24 pt par défaut.
pub fn fit_block(lines: &[String], box_w: f32, box_h: f32, line_spacing: f32) -> f32 {
    fit_block_from(lines, box_w, box_h, line_spacing, 24.0)
}

/// Répartit `lines` en bandes horizontales, du haut vers le bas de `rect`.
///
/// Chaque bande a pour hauteur `size * spacing`. Les bandes sont bornées
/// à l'intérieur de `rect` : si l'empilement dépasse, les dernières
/// bandes sont aplaties et la ligne correspondante est posée de façon
/// dégradée (c'est le cas du plancher 4 pt).
///
/// ponytail: simple empilement, aucune tentative de rééchelonnement ou de
/// césure. Renvoie toujours `lines.len()` rectangles.
pub fn line_strips(lines: &[String], rect: [f32; 4], size: f32, spacing: f32) -> Vec<[f32; 4]> {
    let row = size * spacing;
    (0..lines.len())
        .map(|i| {
            let top = rect[3] - i as f32 * row;
            let bottom = top - row;
            let top = top.min(rect[3]).max(rect[1]);
            let bottom = bottom.max(rect[1]);
            let top = top.max(bottom);
            [rect[0], bottom, rect[2], top]
        })
        .collect()
}

/// Scinde une chaîne de page en lignes, en tronçant les sauts de page et
/// les sauts de ligne, et en supprimant les lignes vides.
fn split_lines(text: &str) -> Vec<String> {
    text.split(['\n', '\u{c}'])
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.trim().is_empty())
        .collect()
}

impl Document {
    /// Applique un caviardage destructif par chirurgie du flux de contenu.
    ///
    /// C'est le moteur principal : il **réécrit** le contenu des pages
    /// concernées plutôt que de les remplacer par une image. Le texte
    /// resté hors des zones caviardées demeure sélectionnable et
    /// cherchable.
    ///
    /// Une page dont le contenu n'est pas découpable proprement est
    /// rasterisée en repli ; `renderer` n'est alors nécessaire que pour
    /// ces pages-là. Si aucune page n'exige de rasterisation, le
    /// caviardage aboutit **sans** pdfium.
    pub fn redact_marks(
        &mut self,
        marks: &[RedactionMark],
        renderer: &dyn Renderer,
    ) -> Result<RedactionReport> {
        let mut report = RedactionReport::default();
        if marks.is_empty() {
            return Ok(report);
        }

        // Regroupement par page : une page peut porter plusieurs zones.
        let mut by_page: std::collections::BTreeMap<usize, Vec<&RedactionMark>> =
            std::collections::BTreeMap::new();
        for mark in marks {
            by_page.entry(mark.page).or_default().push(mark);
        }

        for (page_number, page_marks) in by_page {
            self.redact_page(page_number, &page_marks, renderer, &mut report)?;
        }

        Ok(report)
    }

    /// Caviarde une page : découpe le flux de contenu, ou rasterise.
    fn redact_page(
        &mut self,
        page_number: usize,
        marks: &[&RedactionMark],
        renderer: &dyn Renderer,
        report: &mut RedactionReport,
    ) -> Result<()> {
        let page_id = self
            .page_id(page_number as u32)
            .map_err(|e| Error::Pdf(format!("page {page_number} : {e}")))?;
        let view = self.page_view(page_number as u32);

        // Les zones de l'utilisateur sont en repère affiché ; le flux de
        // contenu travaille en repère page.
        let regions: Vec<geometry::BBox> = marks
            .iter()
            .map(|mark| geometry::BBox::from_rect(view.page_rect(mark.rect)))
            .collect();

        let outcome = surgery::slice_page(self, page_id, &regions)?;

        // Une page non découpable part en rasterisation.
        if outcome.needs_raster {
            self.rasterize_page(page_number, marks, renderer, report)?;
            return Ok(());
        }

        report.text_ops_removed += outcome.text_ops_removed;
        report.vector_ops_removed += outcome.vector_ops_removed;

        // Les images dont la zone recoupe une zone à effacer sont brûlées :
        // les pixels d'origine disparaissent, ils ne sont pas seulement
        // masqués.
        for touched in &outcome.images_touched {
            let overlapping: Vec<geometry::BBox> = regions
                .iter()
                .filter(|region| region.overlaps(&touched.bbox))
                .copied()
                .collect();
            if overlapping.is_empty() {
                continue;
            }
            let fill = marks
                .iter()
                .find(|mark| {
                    let rect = geometry::BBox::from_rect(view.page_rect(mark.rect));
                    rect.overlaps(&touched.bbox)
                })
                .map(|mark| mark.fill)
                .unwrap_or_default();
            let overlay = marks
                .iter()
                .find(|mark| {
                    let rect = geometry::BBox::from_rect(view.page_rect(mark.rect));
                    rect.overlaps(&touched.bbox)
                })
                .and_then(|mark| mark.overlay.as_ref());
            if burn::burn_image(
                self,
                page_number,
                &touched.resource_name,
                &touched.bbox,
                &overlapping,
                fill,
                overlay,
            )? {
                report.images_burned += 1;
            }
        }

        // Le flux réécrit est remis en place. Avant, on y ajoute le
        // remplissage des zones et le texte superposé : sans eux, la page
        // serait simplement amputée de son texte, ce qui n'est pas un
        // caviardage.
        let mut kept = outcome.kept.clone();
        for mark in marks {
            let region = geometry::BBox::from_rect(view.page_rect(mark.rect));
            let painted = surgery::overlay_operations(
                self,
                page_id,
                &region,
                mark.fill,
                mark.overlay.as_ref(),
            )?;
            kept.extend(painted);
        }
        if !kept.is_empty() || outcome.text_ops_removed > 0 || outcome.vector_ops_removed > 0 {
            surgery::write_back(self, page_id, &kept)?;
            report.pages_sliced += 1;
        }

        // Les annotations qui recouvrent une zone caviardée sont
        // détruites : elles peuvent porter le mot sensible.
        let annot_regions: Vec<RedactionRegion> = marks
            .iter()
            .map(|mark| RedactionRegion {
                page: mark.page,
                rect: mark.rect,
            })
            .collect();
        let annot_refs: Vec<&RedactionRegion> = annot_regions.iter().collect();
        let (kept, dropped) = self.classify_annots_for_redaction(page_id, &annot_refs, &view)?;
        if dropped > 0 {
            report.annotations_removed += dropped;
            surgery::set_annots(self, page_id, &kept)?;
        }

        Ok(())
    }

    /// Rasterise une page et y peint les zones caviardées.
    fn rasterize_page(
        &mut self,
        page_number: usize,
        marks: &[&RedactionMark],
        renderer: &dyn Renderer,
        report: &mut RedactionReport,
    ) -> Result<()> {
        // Sans rendu réel, la page deviendrait une image vide : le texte
        // d'origine disparaîtrait sans que la zone sensible soit effacée.
        if !renderer.renders_real_content() {
            return Err(Error::NotSupported(
                "caviardage impossible sans rendu réel : la bibliothèque pdfium est \
                 indisponible, et cette page devrait être rasterisée. Installez pdfium \
                 (voir pdfium/README.md) puis relancez l'opération."
                    .into(),
            ));
        }

        const SCALE: f32 = 2.5;
        let opts = RenderOptions {
            scale: SCALE,
            ..Default::default()
        };
        let bitmap = renderer.render_page(self, page_number as u32, &opts)?;

        let view = self.page_view(page_number as u32);
        let (display_w, display_h) = view.display_size();
        let mut pixels = bitmap.pixels.clone();
        for mark in marks {
            burn::paint_rect(
                &mut pixels,
                bitmap.width,
                bitmap.height,
                display_w,
                display_h,
                mark.rect,
                mark.fill,
            );
            if let Some(overlay) = &mark.overlay {
                let [x1, y1, _x2, _y2] = mark.rect;
                let scale_px = bitmap.height as f32 / display_h.max(1.0);
                burn::draw_text(
                    &mut pixels,
                    bitmap.width,
                    bitmap.height,
                    x1 + 4.0 * scale_px / 2.5,
                    y1 + 4.0 * scale_px / 2.5,
                    &overlay.text,
                    overlay.size * scale_px / 2.5,
                    overlay.color,
                );
            }
        }

        let page_id = self
            .page_id(page_number as u32)
            .map_err(|e| Error::Pdf(format!("page {page_number} : {e}")))?;
        let annot_regions: Vec<RedactionRegion> = marks
            .iter()
            .map(|mark| RedactionRegion {
                page: mark.page,
                rect: mark.rect,
            })
            .collect();
        let annot_refs: Vec<&RedactionRegion> = annot_regions.iter().collect();
        let (kept, dropped) = self.classify_annots_for_redaction(page_id, &annot_refs, &view)?;
        report.annotations_removed += dropped;

        self.replace_page_with_image(
            page_id,
            &pixels,
            bitmap.width,
            bitmap.height,
            &kept,
            &view,
        )?;
        report.pages_rasterized += 1;

        Ok(())
    }

    /// Applique un caviardage destructif.
    ///
    /// `regions` liste toutes les zones à noircir sur toutes les pages.
    /// Les pages sans région restent intactes.
    ///
    /// `renderer` fournit le rendu haute résolution (typiquement
    /// `PdfiumRenderer`).
    ///
    /// Ce moteur **rasterise la page entière** : c'est l'ancien
    /// comportement, conservé tel quel. Le moteur de surgery est
    /// [`Document::redact_marks`].
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

            // 2. Noir sur les régions. Le rendu couvre la boîte visible
            //    avec la rotation appliquée : c'est la géométrie de référence.
            let view = self.page_view(page_num as u32);
            let (display_w, display_h) = view.display_size();
            let mut pixels = bitmap.pixels.clone();
            for region in &page_regions {
                paint_black_rect(
                    &mut pixels,
                    bitmap.width,
                    bitmap.height,
                    display_w,
                    display_h,
                    region.rect,
                );
            }

            // 3. Tri des annotations : celles qui touchent une zone
            // caviardée sont détruites, les autres survivent.
            let (kept, dropped) =
                self.classify_annots_for_redaction(page_id, &page_regions, &view)?;

            // 4. Remplacer le contenu de la page par l'image.
            self.replace_page_with_image(
                page_id,
                &pixels,
                bitmap.width,
                bitmap.height,
                &kept,
                &view,
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
        view: &PageView,
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
                // Un `/Rect` est dans le repère page, une région dans le
                // repère affiché. Comparer l'un à l'autre sans conversion
                // laissait passer, sur une page tournée, une annotation qui
                // recouvre la zone à effacer — donc un mot sensible affiché
                // par-dessus le rectangle noir.
                Some(rect) => {
                    let rect = view.display_rect(rect);
                    regions.iter().any(|r| rects_overlap(rect, r.rect))
                }
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
        view: &PageView,
    ) -> Result<()> {
        // Conversion RGBA → RGB.
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
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

        // Flux de contenu : l'image couvre la boîte visible, rotation
        // comprise. `/Rotate` reste sur la page : les annotations conservées
        // gardent ainsi leur repère, et la page n'est pas déformée.
        let [a, b, c, d, e, f] = view.image_matrix();
        let content = format!("q\n{a} {b} {c} {d} {e} {f} cm\n/Im0 Do\nQ\n");
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

    /// Traduit une page en place, en préservant l'aspect par réduction du corps.
    ///
    /// `translated_lines` est la traduction de la page, déjà découpée en
    /// lignes. La région de texte original est effacée par la primitive de
    /// caviardage (remplissage blanc), et la traduction est posée dans la
    /// même boîte à un corps réduit jusqu'à ce qu'elle tienne (planche 4,0
    /// pt). En mode `bilingual`, les lignes originales sont empilées sous la
    /// traduction, à 0,8× du corps.
    ///
    /// ponytail: page-level blocks only. `rect` est la boîte d'affichage
    /// entière ; les boîtes par fragment de texte nécessiteraient un
    /// extracteur de flux de contenu. Ajouter un tel extracteur si les
    /// documents multi-colonnes ou à tableaux doivent survivre à la
    /// traduction. Les pages sans texte extractible (scans) sont sautées.
    pub fn translate_page(
        &mut self,
        page: u32,
        options: &TranslationOptions,
        translated_lines: &[String],
        renderer: &dyn Renderer,
    ) -> Result<()> {
        let view = self.page_view(page);
        let (w, h) = view.display_size();
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }

        // Une page scannée n'a pas de texte à remplacer : laisser la
        // région en place éviterait de recouvrir l'image de fond.
        let original_text = self.page_text(page).unwrap_or_default();
        if original_text.trim().is_empty() || self.looks_scanned(page) {
            return Ok(());
        }

        let spacing = 1.15f32;
        let box_rect = [0.0, 0.0, w, h];
        let mut marks: Vec<RedactionMark> = Vec::new();

        // La boîte entière est effacée : c'est la primitive de caviardage
        // qui retire le texte original du flux et pose le remplissage.
        marks.push(RedactionMark {
            page: page as usize,
            rect: box_rect,
            fill: RedactionFill::White,
            overlay: None,
        });

        if !translated_lines.is_empty() {
            if options.bilingual {
                let half = h / 2.0;
                let upper = [0.0, half, w, h];
                let lower = [0.0, 0.0, w, half];
                let tsize = fit_block_from(translated_lines, w, half, spacing, 24.0);
                let strips = line_strips(translated_lines, upper, tsize, spacing);
                for (i, line) in translated_lines.iter().enumerate() {
                    if let Some(strip) = strips.get(i) {
                        marks.push(RedactionMark {
                            page: page as usize,
                            rect: *strip,
                            fill: RedactionFill::White,
                            overlay: Some(RedactionOverlay {
                                text: line.clone(),
                                font: PatchFont::Helvetica,
                                size: tsize,
                                color: [0, 0, 0],
                            }),
                        });
                    }
                }
                let osize = (tsize * 0.8).max(4.0);
                let original_lines = split_lines(&original_text);
                let ostrips = line_strips(&original_lines, lower, osize, spacing);
                for (i, line) in original_lines.iter().enumerate() {
                    if let Some(strip) = ostrips.get(i) {
                        marks.push(RedactionMark {
                            page: page as usize,
                            rect: *strip,
                            fill: RedactionFill::White,
                            overlay: Some(RedactionOverlay {
                                text: line.clone(),
                                font: PatchFont::Helvetica,
                                size: osize,
                                color: [0, 0, 0],
                            }),
                        });
                    }
                }
            } else {
                let tsize = fit_block_from(translated_lines, w, h, spacing, 24.0);
                let strips = line_strips(translated_lines, box_rect, tsize, spacing);
                for (i, line) in translated_lines.iter().enumerate() {
                    if let Some(strip) = strips.get(i) {
                        marks.push(RedactionMark {
                            page: page as usize,
                            rect: *strip,
                            fill: RedactionFill::White,
                            overlay: Some(RedactionOverlay {
                                text: line.clone(),
                                font: PatchFont::Helvetica,
                                size: tsize,
                                color: [0, 0, 0],
                            }),
                        });
                    }
                }
            }
        }

        self.redact_marks(&marks, renderer)?;
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

/// Remplit un rectangle en noir dans un bitmap RGBA.
///
/// Le rectangle est fourni en coordonnées PDF (origine bas-gauche).
/// Le bitmap est en coordonnées écran (origine haut-gauche).
fn paint_black_rect(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    display_w: f32,
    display_h: f32,
    rect: [f32; 4],
) {
    let [x1, y1, x2, y2] = rect;
    let (rx1, rx2) = (x1.min(x2), x1.max(x2));
    let (ry1, ry2) = (y1.min(y2), y1.max(y2));

    // Le rectangle est dans le repère affiché : on le ramène en fractions de
    // la page, puis en pixels de l'image réellement rendue (dont la taille
    // peut différer d'un pixel de `taille × échelle`).
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
    // L'axe y est inversé : PDF bas-gauche, pixels haut-gauche.
    let py1 = ((1.0 - fy(ry2)) * height as f32).clamp(0.0, height as f32) as u32;
    let py2 = ((1.0 - fy(ry1)) * height as f32).clamp(0.0, height as f32) as u32;

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
}

#[cfg(test)]
mod translate_tests {
    use super::{fit_block, fit_block_from, line_strips, zip_lines};
    use crate::create::blank_document;

    /// Le réducteur de corps baisse jusqu'à ce que chaque ligne tienne, et
    /// s'arrête au plancher de 4,0 pt. Une boîte trop petite pour un seul
    /// mot long reste au plancher, jamais en dessous.
    #[test]
    fn fit_block_respects_floor_and_width() {
        let lines = vec!["un mot tres long qui ne tient pas".to_string()];
        // Boîte étroite : le texte ne tient même pas à 4 pt.
        assert_eq!(fit_block(&lines, 10.0, 100.0, 1.0), 4.0);
        // Boîte très large : le texte tient à 24 pt, on ne descend pas.
        assert_eq!(fit_block(&lines, 500.0, 100.0, 1.0), 24.0);
        // Boîte moyenne : on descend d'un cran, sans passer sous le plancher.
        let size = fit_block(&lines, 120.0, 100.0, 1.0);
        assert!(size >= 4.0 && size < 24.0);
        // Une ligne vide renvoie le plancher.
        assert_eq!(fit_block(&[], 100.0, 100.0, 1.0), 4.0);
    }

    /// `fit_block_from` part d'un corps donné : plus on commence haut, plus
    /// on peut rester haut si le texte tient.
    #[test]
    fn fit_block_from_start_matters() {
        let lines = vec!["court".to_string()];
        assert_eq!(fit_block_from(&lines, 400.0, 100.0, 1.0, 12.0), 12.0);
        assert_eq!(fit_block_from(&lines, 400.0, 100.0, 1.0, 24.0), 24.0);
    }

    /// Les bandes sont empilées du haut vers le bas, bornées à l'intérieur
    /// de la boîte, et il y en a autant que de lignes.
    #[test]
    fn line_strips_stack_top_down() {
        let rect = [0.0, 0.0, 100.0, 100.0];
        let strips = line_strips(&["a".to_string(), "b".to_string(), "c".to_string()], rect, 10.0, 1.0);
        assert_eq!(strips.len(), 3);
        // La première ligne est collée au haut de la boîte.
        assert!((strips[0][3] - 100.0).abs() < 0.001);
        // Les lignes sont empilées vers le bas, sans chevauchement.
        assert!((strips[0][1] - 90.0).abs() < 0.001);
        assert!((strips[1][3] - 90.0).abs() < 0.001);
        assert!((strips[2][1] - 70.0).abs() < 0.001);
        // Les bandes ne débordent pas la boîte.
        for s in &strips {
            assert!(s[0] >= rect[0] - 0.001);
            assert!(s[2] <= rect[2] + 0.001);
            assert!(s[1] >= rect[1] - 0.001);
            assert!(s[3] <= rect[3] + 0.001);
        }
    }

    /// `zip_lines` zippe par position, pas par correspondance de contenu.
    #[test]
    fn zip_lines_pairs_by_index() {
        let original = vec!["a".to_string(), "b".to_string()];
        let translated = vec!["A".to_string()];
        let pairs = zip_lines(&original, &translated);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, &"a".to_string());
        assert_eq!(pairs[0].1, &"A".to_string());
    }

    /// Une page sans texte n'a rien à traduire : `translate_page` est un
    /// chemin de production, on le vérifie par l'absence d'erreur sur un
    /// document vide.
    #[test]
    fn translate_blank_page_is_noop() {
        let mut doc = blank_document(1, 595.0, 842.0).expect("document vide");
        let opts = crate::TranslationOptions {
            target_language: "English".to_string(),
            bilingual: false,
            preserve_layout: true,
        };
        let renderer = crate::render::NullRenderer;
        // Une page blanche n'a pas de texte : l'opération est un no-op.
        let result = doc.translate_page(1, &opts, &["bonjour".to_string()], &renderer);
        assert!(result.is_ok());
    }
}
