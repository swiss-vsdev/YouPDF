//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel compare_ui).


use super::*;

impl YouPdfApp {

        /// Fenêtre « Comparer des PDF » : différences de texte entre deux fichiers.
        pub(super) fn compare_window(&mut self, ctx: &egui::Context) {
            if !self.compare_open {
                return;
            }
            let mut open = true;
            let mut pick = false;
            let mut run = false;
            let mut jump: Option<usize> = None;
            egui::Window::new(t("Compare PDFs"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(vec2(900.0, 600.0))
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .frame(modal_frame(ctx))
                .show(ctx, |ui| {
                    self.compare_controls(ui, &mut pick, &mut run);
                    ui.add_space(SPACE_L);
                    // La vue côte à côte n'apparaît qu'une fois les deux documents chargés.
                    if self.compare_tab.is_some() {
                        ui.add_space(SPACE_S);
                        ui.separator();
                        ui.add_space(SPACE_S);
                        // Côte à côte et superposée partagent la navigation ; la
                        // superposée n'est qu'un autre tracé du même panneau.
                        self.compare_split_view(ui);
                    }
                    if let Some(report) = &self.compare_report {
                        if self.compare_view == CompareView::Report {
                            ui.add_space(SPACE_L);
                            ui.separator();
                            ui.add_space(SPACE_M);
                            ui.label(
                                RichText::new(format!(
                                    "{} page(s) vs {} page(s) — {} difference(s), \
                                     {} identical line(s)",
                                    report.pages_a,
                                    report.pages_b,
                                    report.differences.len(),
                                    report.identical_lines
                                ))

                                .color(pal().icon_bright),
                            );
                            ui.add_space(SPACE_S);
                            ui.horizontal(|ui| {
                                for (label, color) in [
                                    ("Inserted", ADDED_GREEN),
                                    ("Deleted", REMOVED_RED),
                                    ("Modified", MODIFIED_AMBER),
                                ] {
                                    ui.label(
                                        RichText::new(format!("\u{25a0} {label}"))
                                            .size(CAPTION)
                                            .color(color),
                                    );
                                }
                            });
                            ui.add_space(SPACE_S);
                            ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                for diff in report.differences.iter().take(300) {
                                    let (tag, color) = match diff.kind {
                                        DifferenceKind::Added => ("+", ADDED_GREEN),
                                        DifferenceKind::Removed => ("-", REMOVED_RED),
                                        DifferenceKind::Modified => ("~", MODIFIED_AMBER),
                                        DifferenceKind::PageAdded => ("+ page", ADDED_GREEN),
                                        DifferenceKind::PageRemoved => ("- page", REMOVED_RED),
                                    };
                                    let text = diff
                                        .after
                                        .clone()
                                        .or_else(|| diff.before.clone())
                                        .unwrap_or_default();
                                    let page = diff.page_b.or(diff.page_a).unwrap_or(0);
                                    if ui
                                        .selectable_label(
                                            false,
                                            RichText::new(format!("{tag}  p{page}  {text}"))
                                                .size(LABEL)
                                                .color(color),
                                        )
                                        .clicked()
                                    {
                                        jump = Some(page);
                                    }
                                }
                            });
                        }
                    }
                });

            if let Some(page) = jump {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if page >= 1 {
                        tab.page = page.min(tab.total());
                    }
                }
            }
            if pick {
                self.compare_path = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_file();
                self.compare_report = None;
            }
            if run {
                match self.compare_path.clone() {
                    Some(path) => match Document::open(&path) {
                        Ok(other) => {
                            let report = self
                                .tabs
                                .get(self.active)
                                .map(|tab| {
                                tab.doc.compare_with_options(
                                    &other,
                                    self.compare_ignore_ws,
                                    self.compare_ignore_case,
                                )
                            });
                            match report {
                                Some(Ok(report)) => {
                                    let identical = report.is_identical();
                                    self.compare_report = Some(report);
                                    self.compare_tab = Some(Tab::from_document(other, Some(path)));
                                    self.compare_view = CompareView::SideBySide;
                                    self.compare_page = 1;
                                    if identical {
                                        self.toast(ctx, "The two documents match");
                                    }
                                }
                                Some(Err(err)) => self.toast(ctx, format!("Compare failed: {err}")),
                                None => self.toast(ctx, "Open a document first"),
                            }
                        }
                        Err(err) => self.toast(ctx, format!("Open failed: {err}")),
                    },
                    None => self.toast(ctx, "Choose a PDF to compare with"),
                }
            }
            if !open {
                self.compare_open = false;
            }
        }

        /// Vue côte à côte des deux documents, navigation par page.
        pub(super) fn compare_split_view(&mut self, ui: &mut egui::Ui) {
            // Les totaux sont lus avant d'emprunter `self` mutablement : les
            // closures du ScrollArea ont besoin de `&mut self` pour rastériser
            // chaque panneau, et deux emprunts mutables côte à côte ne coexistent
            // pas.
            let total_a = self
                .tabs
                .get(self.active)
                .map(|t| t.total())
                .unwrap_or(0);
            let total_b = self
                .compare_tab
                .as_ref()
                .map(|t| t.total())
                .unwrap_or(0);
            let total = total_a.max(total_b);
            if total == 0 {
                return;
            }

            // Page courante bornée.
            self.compare_page = self.compare_page.clamp(1, total);

            // Contrôles de navigation.
            ui.horizontal(|ui| {
                if ui.small_button(t("First")).clicked() {
                    self.compare_page = 1;
                }
                if ui.small_button(t("Prev")).clicked() {
                    self.compare_page = self.compare_page.saturating_sub(1);
                }
                let resp = ui.add(
                    egui::DragValue::new(&mut self.compare_page)
                        .speed(1.0)
                        .range(1..=total)
                        .prefix(""),
                );
                if resp.changed() {
                    self.compare_page = self.compare_page.clamp(1, total);
                }
                if ui.small_button(t("Next")).clicked() {
                    self.compare_page = (self.compare_page + 1).min(total);
                }
                if ui.small_button(t("Last")).clicked() {
                    self.compare_page = total;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button(t("Text report")).clicked() {
                        self.compare_view = CompareView::Report;
                    }
                });
            });
            ui.add_space(SPACE_S);
            ui.separator();
            ui.add_space(SPACE_S);

            // Les deux rasters sont produits avant l'affichage : chaque panneau
            // emprunte son propre onglet et les deux ne peuvent pas être tenus
            // en même temps.
            let page_a = self.compare_page.min(total_a);
            let page_b = self.compare_page.min(total_b);
            let zoom_a = self.tabs.get(self.active).and_then(|t| t.zoom).unwrap_or(1.0);
            let zoom_b = self.compare_tab.as_ref().and_then(|t| t.zoom).unwrap_or(1.0);
            let ppp = ui.ctx().pixels_per_point();
            let mut status = String::new();
            let tex_a = self.tabs.get_mut(self.active).and_then(|tab| {
                YouPdfApp::page_texture(
                    ui.ctx(),
                    &*self.renderer,
                    tab,
                    page_a as u32,
                    render_step(zoom_a, ppp),
                    zoom_a,
                    ppp,
                    true,
                    false,
                    &mut status,
                )
            });
            let tex_b = self.compare_tab.as_mut().and_then(|tab| {
                YouPdfApp::page_texture(
                    ui.ctx(),
                    &*self.renderer,
                    tab,
                    page_b as u32,
                    render_step(zoom_b, ppp),
                    zoom_b,
                    ppp,
                    true,
                    false,
                    &mut status,
                )
            });
            let diffs_a = self.diffs_on_page(true, page_a);
            let diffs_b = self.diffs_on_page(false, page_b);
            let blend = self.compare_blend;

            let available = ui.available_size();

            if self.compare_view == CompareView::Overlay {
                // Le second document est posé sur le premier à l'opacité réglée
                // dans la barre de contrôle : les décalages sautent aux yeux.
                let (rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
                if let Some(tex) = &tex_a {
                    let img_rect = compare_fit_rect(rect, tex.size_vec2());
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                if let Some(tex) = &tex_b {
                    let img_rect = compare_fit_rect(rect, tex.size_vec2());
                    ui.painter().image(
                        tex.id(),
                        img_rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::from_white_alpha((blend.clamp(0.0, 1.0) * 255.0) as u8),
                    );
                }
                return;
            }

            let pane_width = available.x / 2.0;
            ui.horizontal(|ui| {
                for (label, tex, diffs) in [
                    (t("Document A"), tex_a, diffs_a),
                    (t("Document B"), tex_b, diffs_b),
                ] {
                    ui.allocate_ui_with_layout(
                        vec2(pane_width, available.y),
                        Layout::top_down(Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(label)

                                    .color(pal().icon_bright),
                            );
                            let Some(tex) = tex else { return };
                            let (slot, _) = ui.allocate_exact_size(
                                vec2(pane_width, available.y - 24.0),
                                egui::Sense::hover(),
                            );
                            let img_rect = compare_fit_rect(slot, tex.size_vec2());
                            ui.painter().image(
                                tex.id(),
                                img_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                            if diffs > 0 {
                                // Liseré + compteur : les différences du rapport
                                // sont signalées page par page.
                                ui.painter().rect_stroke(
                                    img_rect.expand(2.0),
                                    Rounding::same(RADIUS_XS),
                                    Stroke::new(1.5_f32, MODIFIED_AMBER),
                                );
                                ui.painter().text(
                                    img_rect.left_top() + vec2(6.0, 4.0),
                                    Align2::LEFT_TOP,
                                    format!("{diffs}"),
                                    FontId::proportional(LABEL),
                                    MODIFIED_AMBER,
                                );
                            }
                        },
                    );
                    ui.add_space(SPACE_S);
                }
            });
        }

}
