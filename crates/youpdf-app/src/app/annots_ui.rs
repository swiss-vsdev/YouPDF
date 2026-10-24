//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel annots_ui).

use super::*;

impl YouPdfApp {

        /// Charge les annotations du document à la première utilisation.
        pub(super) fn ensure_annots(&mut self) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if !tab.annots_loaded {
                    tab.annots = annots_to_display(&tab.doc, tab.doc.read_annotations());
                    tab.annots_loaded = true;
                }
            }
        }

        /// Style courant pour un type d'annotation donné.
        pub(super) fn annot_style(&self, kind: AnnotationKind) -> AnnotationStyle {
            let mut style = AnnotationStyle {
                color: self.annot_color,
                opacity: 1.0,
                line_width: self.annot_width,
                font_size: 12.0,
                fill: None,
                dash: BorderDash::Solid,
                cloudy: false,
            };
            let _ = &mut style;
            style.opacity = match kind {
                AnnotationKind::Highlight | AnnotationKind::Squiggly | AnnotationKind::Underline
                | AnnotationKind::StrikeOut => self.annot_opacity,
                AnnotationKind::Ink | AnnotationKind::Square | AnnotationKind::Circle
                | AnnotationKind::Line | AnnotationKind::Arrow => 1.0,
                _ => 1.0,
            };
            if kind == AnnotationKind::Note {
                style.font_size = 10.0;
            }
            style
        }

        /// Ajoute une annotation et l'inscrit dans l'historique.
        pub(super) fn push_annot(&mut self, annot: Annotation) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                let before = tab.annots.clone();
                tab.record_annots(before);
                tab.annots.push(annot);
                tab.dirty = true;
            }
        }

        /// Retire l'annotation d'indice donné.
        pub(super) fn remove_annot(&mut self, index: usize) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if index < tab.annots.len() {
                    let before = tab.annots.clone();
                    tab.record_annots(before);
                    tab.annots.remove(index);
                    tab.dirty = true;
                }
            }
            self.has_annot_selection = false;
        }

        /// Écrit les annotations dans le document avant enregistrement.
        pub(super) fn flush_annots(&mut self) -> std::result::Result<(), String> {
            self.ensure_annots();
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return Ok(());
            };
            if tab.annots.is_empty() {
                return Ok(());
            }
            let page_annots = annots_to_page(&tab.doc, &tab.annots);
            tab.doc
                .apply_annotations(&page_annots)
                .map_err(|err| err.to_string())
        }

        /// Type de brouillon pour l'outil courant (texte, note, tampon, lien).
        pub(super) fn draft_kind(&self) -> DraftKind {
            draft_kind_for(self.mode, self.annot_tool)
        }

        /// Ajoute une annotation de type rectangle (surlignage, formes).
        pub(super) fn add_rect_annot(&mut self, page: usize, kind: AnnotationKind, rect: [f32; 4]) {
            if (rect[2] - rect[0]).abs() < 2.0 && (rect[3] - rect[1]).abs() < 2.0 {
                return;
            }
            // Une ligne ou une flèche, c'est deux extrémités : le rectangle n'est
            // que leur boîte englobante, et il ne faut donc pas l'épaissir comme
            // une bande de surlignage.
            if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) {
                let mut annot = Annotation::new(page, kind, rect);
                annot.style = self.annot_style(kind);
                annot.add_ink_stroke(vec![[rect[0], rect[1]], [rect[2], rect[3]]]);
                annot.recompute_ink_bbox();
                self.push_annot(annot);
                return;
            }
            // Un glissé horizontal (surlignage) donne une bande de hauteur nulle :
            // on l'épaissit autour de la ligne visée.
            let mut rect = rect;
            if (rect[3] - rect[1]).abs() < 3.0 {
                let middle = (rect[1] + rect[3]) / 2.0;
                rect[1] = middle - 6.0;
                rect[3] = middle + 6.0;
            }
            let mut annot = Annotation::new(page, kind, rect);
            annot.style = self.annot_style(kind);
            if kind == AnnotationKind::Stamp {
                let template = &STAMP_TEMPLATES[self.stamp_choice.min(STAMP_TEMPLATES.len() - 1)];
                let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                let author = self.comment_author.clone().unwrap_or_default();
                annot.content = youpdf_core::annotate::dynamic_stamp_text(template, &author, &date);
                annot.style.color = template.color;
                annot.style.font_size = youpdf_core::annotate::STAMP_FONT_SIZE;
                annot.stamp = Some(template.id.to_string());
            }
            self.push_annot(annot);
        }

        /// Ajoute une mise en évidence alignée sur les lignes de texte.
        pub(super) fn add_markup_annot(&mut self, page: usize, kind: AnnotationKind, quads: Vec<[f32; 8]>) {
            if quads.is_empty() {
                return;
            }
            let mut annot = Annotation::new(page, kind, [0.0, 0.0, 0.0, 0.0]);
            annot.style = self.annot_style(kind);
            annot.set_quads(quads);
            self.push_annot(annot);
        }

        /// Ajoute un tracé de crayon.
        pub(super) fn add_ink_annot(&mut self, page: usize, points: Vec<[f32; 2]>) {
            if points.len() < 2 {
                return;
            }
            let mut annot = Annotation::new(page, AnnotationKind::Ink, [0.0, 0.0, 0.0, 0.0]);
            annot.style = self.annot_style(AnnotationKind::Ink);
            annot.add_ink_stroke(points);
            annot.recompute_ink_bbox();
            self.push_annot(annot);
        }

        /// Ouvre le brouillon de saisie pour un clic sur la page.
        pub(super) fn open_draft(&mut self, page: usize, point: [f32; 2]) {
            let kind = self.draft_kind();
            let draft_kind = self.draft_kind();
            if draft_kind == DraftKind::Signature {
                // Une signature visuelle est une image, pas une annotation : le
                // créateur la fabrique et la pose au point cliqué.
                self.marks.signature_at = Some((page, point));
                self.marks.signature_open = true;
                return;
            }
            let (annot_kind, rect, buffer) = match draft_kind {
                DraftKind::Note => (
                    AnnotationKind::Note,
                    [point[0], point[1] - 20.0, point[0] + 20.0, point[1]],
                    String::new(),
                ),
                DraftKind::Stamp => {
                    let template = &STAMP_TEMPLATES[self.stamp_choice.min(STAMP_TEMPLATES.len() - 1)];
                    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let author = self.comment_author.clone().unwrap_or_default();
                    let text = youpdf_core::annotate::dynamic_stamp_text(template, &author, &date);
                    (
                        AnnotationKind::Stamp,
                        [point[0], point[1] - 26.0, point[0] + 150.0, point[1] + 26.0],
                        text,
                    )
                }
                DraftKind::Link => (
                    AnnotationKind::Link,
                    [point[0], point[1] - 12.0, point[0] + 150.0, point[1] + 12.0],
                    String::new(),
                ),
                DraftKind::Signature => (
                    AnnotationKind::FreeText,
                    [point[0], point[1] - 18.0, point[0] + 170.0, point[1] + 18.0],
                    String::new(),
                ),
                DraftKind::FreeText => (
                    AnnotationKind::FreeText,
                    [point[0], point[1] - 14.0, point[0] + 180.0, point[1] + 22.0],
                    String::new(),
                ),
            };
            let _ = kind;
            self.text_draft = Some(TextDraft {
                kind: draft_kind,
                page,
                rect,
                buffer,
                fresh: true,
                caret_at: None,
                single_line: true,
            });
            self.draft_annot_kind = annot_kind;
            if draft_kind == DraftKind::Stamp {
                self.confirm_draft();
            }
        }

        /// Valide le brouillon courant.
        pub(super) fn confirm_draft(&mut self) {
            let Some(draft) = self.text_draft.take() else {
                return;
            };
            let kind = self.draft_annot_kind;
            if draft.buffer.trim().is_empty() && draft.kind != DraftKind::Stamp {
                return;
            }
            let mut annot = Annotation::new(draft.page, kind, draft.rect);
            annot.content = draft.buffer.trim().to_owned();
            annot.style = self.annot_style(kind);
            if kind == AnnotationKind::FreeText {
                match draft.kind {
                    DraftKind::Signature => {
                        annot.style.font_size = 22.0;
                        annot.style.color = [20, 20, 20];
                    }
                    _ => {
                        annot.style.font_size = 13.0;
                        annot.style.color = self.annot_color;
                    }
                }
            }
            if kind == AnnotationKind::Link {
                annot.style.color = [40, 96, 200];
            }
            self.push_annot(annot);
        }

        /// Zone réagissant aux clics pour les annulations/sélections.
        pub(super) fn hit_test(&self, page: usize, point: [f32; 2]) -> Option<usize> {
            let tab = self.tabs.get(self.active)?;
            tab.annots
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| {
                    a.page == page
                        && a.reply_to.is_none()
                        && point[0] >= a.rect[0] - 4.0
                        && point[0] <= a.rect[2] + 4.0
                        && point[1] >= a.rect[1] - 4.0
                        && point[1] <= a.rect[3] + 4.0
                })
                .map(|(idx, _)| idx)
        }

        /// Indice du tracé de crayon réellement touché à `point`, en ignorant les
        /// boîtes englobantes vides.
        pub(super) fn ink_hit_test(&self, page: usize, point: [f32; 2]) -> Option<usize> {
            let tab = self.tabs.get(self.active)?;
            tab.annots
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| {
                    a.page == page
                        && a.kind == AnnotationKind::Ink
                        && a.reply_to.is_none()
                        && a.ink_hit(point, 8.0)
                })
                .map(|(idx, _)| idx)
        }

        /// Gomme mode segment : coupe les tracés de `page` qui croisent le
        /// segment `[from, to]`. Les tracés entièrement balayés disparaissent.
        pub(super) fn erase_ink_segment(&mut self, page: usize, from: [f32; 2], to: [f32; 2], ctx: &egui::Context) {
            /// Rayon du pinceau de la gomme, en points PDF.
            const RADIUS: f32 = 6.0;
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let before = tab.annots.clone();
            let mut changed = false;
            let mut kept: Vec<Annotation> = Vec::with_capacity(tab.annots.len());
            for mut annot in std::mem::take(&mut tab.annots) {
                if annot.page == page && annot.kind == AnnotationKind::Ink
                    && annot.erase_ink_segment(from, to, RADIUS) > 0
                {
                    changed = true;
                    if annot.ink_points.is_empty() {
                        continue;
                    }
                    annot.recompute_ink_bbox();
                }
                kept.push(annot);
            }
            tab.annots = kept;
            if changed {
                tab.record_annots(before);
                tab.dirty = true;
                self.toast(ctx, "Ink erased");
            }
        }

        /// Couleur du fond de page autour d'un bloc, en repère affiché.
        pub(super) fn mask_color(&self, page: usize, rect: [f32; 4]) -> [u8; 3] {
            match self.tabs.get(self.active) {
                Some(tab) => mask_color_in(tab, page, rect),
                None => [255, 255, 255],
            }
        }

        /// Pose une mise en évidence sur la sélection de texte courante.
        ///
        /// C'est le geste du lecteur : on sélectionne le texte, puis on le
        /// colore. La marque suit les lignes du texte sélectionné — un
        /// quadrilatère par ligne — et non un rectangle posé à l'aveugle.
        pub(super) fn markup_selection(&mut self, kind: AnnotationKind, ctx: &egui::Context) {
            let Some(sel) = self.text_sel else {
                return;
            };
            // Une sélection sans ancre couvre toute la page : c'est le geste
            // « tout sélectionner », pas une sélection de lecture.
            if sel.span.is_none() {
                self.toast(ctx, "Select some text first");
                return;
            }
            let quads = self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.fragments.get(&sel.page))
                .map(|frags| hit_quads(frags, sel.span))
                .unwrap_or_default();
            if quads.is_empty() {
                self.toast(ctx, "No text selected");
                return;
            }
            self.add_markup_annot(sel.page as usize, kind, quads);
            self.text_sel = None;
            let label = MARKUP_KINDS
                .iter()
                .find(|(candidate, _)| *candidate == kind)
                .map(|(_, label)| *label)
                .unwrap_or("Mark");
            self.toast(ctx, format!("{label} added"));
        }

        pub(super) fn draft_window(&mut self, ctx: &egui::Context) {
            let Some(draft) = self.text_draft.as_mut() else {
                return;
            };
            let title = match draft.kind {
                DraftKind::Note => "Add note",
                DraftKind::FreeText => "Add text",
                DraftKind::Signature => "Add signature",
                DraftKind::Link => "Add link",
                DraftKind::Stamp => "Stamp text",
            };
            let hint = match draft.kind {
                DraftKind::Link => "https://…",
                DraftKind::Signature => "Type your name",
                _ => "Text…",
            };
            let fresh = draft.fresh;
            draft.fresh = false;
            let buffer = draft.buffer.clone();

            let mut open = true;
            self.show_modal(ctx, "modal-draft", title, 300.0, &mut open, |app, ui| {
                let Some(draft) = app.text_draft.as_mut() else {
                    return;
                };
                draft.buffer = buffer;
                let resp = ui.add_sized(
                    vec2(280.0, 26.0),
                    TextEdit::singleline(&mut draft.buffer).hint_text(hint),
                );
                if fresh {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.confirm_draft();
                    app.text_draft = None;
                }
                ui.horizontal(|ui| {
                    if ui.button(t("Add")).clicked() {
                        app.confirm_draft();
                        app.text_draft = None;
                    }
                    if ui.button(t("Cancel")).clicked() {
                        app.text_draft = None;
                    }
                });
            });
            if !open {
                self.text_draft = None;
            }
        }

        /// Type de marque ou de forme associé à un outil du menu.
        pub(super) fn annot_kind_for(&self, tool: usize) -> AnnotationKind {
            match tool {
                1 => self.marker,
                5 => self.shape,
                _ => AnnotationKind::Highlight,
            }
        }

        /// Choisit le type de marque ou de forme d'un outil.
        pub(super) fn set_annot_kind(&mut self, tool: usize, kind: AnnotationKind) {
            match tool {
                1 => self.marker = kind,
                5 => {
                    self.shape = kind;
                    // Un trait fin se règle en points, pas en bande de
                    // surlignage : la largeur courante serait énorme.
                    if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) && self.annot_width > 4.0
                    {
                        self.annot_width = 2.0;
                    }
                }
                _ => {}
            }
        }

        /// Marque toutes les correspondances pour caviardage et ouvre le mode.
        ///
        /// `confirm` ouvre directement la fenêtre d'application : c'est le
        /// comportement de « supprimer les correspondances », qui doit être
        /// destructif pour être honnête (masquer le texte ne le supprime pas).
        pub(super) fn mark_matches_for_redaction(&mut self, ctx: &egui::Context, confirm: bool) {
            let Some(needle) = self.search_needle() else {
                return;
            };
            let options = SearchOptions {
                case_sensitive: self.search_case,
                whole_word: self.search_word,
            };
            let pages: std::collections::BTreeSet<usize> =
                self.results.iter().map(|result| result.page).collect();
            let mut added = 0usize;
            {
                let Some(tab) = self.tabs.get(self.active) else {
                    return;
                };
                for page in pages {
                    let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                        continue;
                    };
                    for fragment in fragments {
                        let rect = youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect);
                        for rect in match_rects(&fragment.text, rect, &needle, options) {
                            // Ne pas empiler deux fois la même marque : le bouton
                            // corbeille et le bouton surlignage peuvent se suivre.
                            // La déduplication porte sur la page et le rectangle :
                            // le remplissage et le texte superposé ne font pas
                            // partie de l'identité de la zone.
                            if !self
                                .redact_marks
                                .iter()
                                .any(|m| m.page == page && m.rect == rect)
                            {
                                let fill = self.redact_fill;
                                let overlay = self.overlay_for(fill);
                                self.redact_marks.push(RedactionMark {
                                    page,
                                    rect,
                                    fill,
                                    overlay,
                                });
                                added += 1;
                            }
                        }
                    }
                }
            }
            if added == 0 {
                self.toast(ctx, "Nothing to mark");
                return;
            }
            self.mode = Some(3);
            self.annot_tool = Some(1);
            self.panel = Some(Panel::Search);
            self.redact_modal = confirm;
            if confirm {
                self.toast(ctx, format!("{added} match(es) marked for deletion"));
            } else {
                self.toast(ctx, format!("{added} area(s) marked — press Apply"));
            }
        }

        /// Le texte à superposer sur une zone, s'il y en a un.
        ///
        /// La couleur suit le fond : blanc sur un remplissage sombre, noir sur
        /// un remplissage clair, sinon le texte superposé serait illisible.
        pub(super) fn overlay_for(&self, fill: RedactionFill) -> Option<RedactionOverlay> {
            let text = self.redact_overlay_text.trim();
            if text.is_empty() {
                return None;
            }
            let background = match fill {
                RedactionFill::Black => [0, 0, 0],
                RedactionFill::White => [255, 255, 255],
                RedactionFill::Custom(rgb) => rgb,
            };
            // Luminance relative : au-delà de la moitié, le fond est clair.
            let luminance = 0.299 * f32::from(background[0])
                + 0.587 * f32::from(background[1])
                + 0.114 * f32::from(background[2]);
            let color = if luminance > 140.0 {
                [0, 0, 0]
            } else {
                [255, 255, 255]
            };
            Some(RedactionOverlay {
                text: text.to_string(),
                font: self.redact_overlay_font,
                size: self.redact_overlay_size,
                color,
            })
        }

        /// Applique le caviardage marqué (destructif).
        pub(super) fn apply_redactions(&mut self, ctx: &egui::Context) {
            let marks = std::mem::take(&mut self.redact_marks);
            self.redact_modal = false;
            self.redact_phase = RedactPhase::Marking;
            if marks.is_empty() {
                self.toast(ctx, "No areas marked");
                return;
            }
            let sanitize = self.redact_sanitize;
            let outcome: Result<(RedactionReport, usize), String> = {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                let renderer = &*self.renderer;
                match tab.doc.redact_marks(&marks, renderer) {
                    Ok(report) => {
                        let cleaned = if sanitize {
                            tab.doc.sanitize().unwrap_or(0)
                        } else {
                            0
                        };
                        Ok((report, cleaned))
                    }
                    Err(err) => Err(err.to_string()),
                }
            };
            // Le caviardage réécrit les pages : caches et instantanés caducs.
            self.mark_document_changed();
            match outcome {
                Ok((report, cleaned)) => {
                    // Le caviardage a détruit le contenu sous les zones : les
                    // annotations en mémoire sont caduques. Sans ce rechargement,
                    // `save_with_annotations` réécrirait dans le fichier une
                    // annotation qui chevauchait une zone caviardée.
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.annots = annots_to_display(&tab.doc, tab.doc.read_annotations());
                        tab.annots_loaded = true;
                    }
                    self.selected_annot = 0;
                    self.has_annot_selection = false;
                    self.annot_tool = Some(0);
                    // Le bilan est honnête : on n'annonce une rasterisation que
                    // s'il y en a eu une, mais on dit ce qui a été fait.
                    let mut message = format!(
                        "Redaction applied ({} areas) · {} pages edited",
                        marks.len(),
                        report.pages_sliced
                    );
                    if report.pages_rasterized > 0 {
                        message.push_str(&format!(", {} page(s) rasterized", report.pages_rasterized));
                    }
                    if sanitize {
                        message.push_str(&format!(" · {cleaned} metadata entries removed"));
                    }
                    self.toast(ctx, message);
                }
                Err(err) => {
                    self.redact_marks = marks;
                    self.toast(ctx, format!("Redaction failed: {err}"));
                }
            }
        }

        /// Peint les annotations dans les pages du document ouvert.
        ///
        /// L'opération est définitive pour la session : le dessin devient du
        /// contenu de page, les annotations disparaissent — y compris de
        /// l'historique, qui ne saurait plus les remettre en état sans les
        /// réintroduire dans le PDF au moment de l'enregistrement.
        pub(super) fn flatten_annotations(&mut self, ctx: &egui::Context) {
            // Les annotations vivent dans l'onglet jusqu'à l'enregistrement : il
            // faut d'abord les écrire dans le document pour pouvoir les aplatir.
            if let Err(err) = self.flush_annots() {
                self.toast(ctx, format!("Flatten failed: {err}"));
                return;
            }
            if let Some(tab) = self.tabs.get_mut(self.active) {
                // Le flush vient d'écrire les annotations dans les octets : une
                // image d'avant ne saurait plus les retirer sans les réintroduire.
                // Aplatir reste donc définitif pour l'historique.
                tab.history_base = None;
            }
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let result = tab.doc.flatten_annotations();
            let count = match result {
                Ok(count) => count,
                Err(err) => {
                    self.toast(ctx, format!("Flatten failed: {err}"));
                    return;
                }
            };
            if count == 0 {
                self.toast(ctx, "Nothing to flatten");
                return;
            }
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.annots.clear();
                tab.annots_loaded = true;
            }
            self.selected_annot = 0;
            self.has_annot_selection = false;
            self.mark_document_changed();
            self.toast(ctx, format!("{count} annotation(s) flattened"));
        }

        /// Nombre de différences signalées sur une page, selon le côté du
        /// rapport (A ou B).
        pub(super) fn diffs_on_page(&self, side_a: bool, page: usize) -> usize {
            let Some(report) = &self.compare_report else {
                return 0;
            };
            report
                .differences
                .iter()
                .filter(|d| {
                    let p = if side_a { d.page_a } else { d.page_b };
                    p == Some(page)
                })
                .count()
        }

}
