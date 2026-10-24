//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel search_ui).


use super::*;

impl YouPdfApp {

        pub(super) fn search_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            // Rangée d'actions : recherche · Aa (remplacer) · corbeille · surlignage.
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if panel_action(ui, true, icon_search).clicked() {
                    self.run_search(ctx);
                }

                let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
                if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, Rounding::same(RADIUS_M), Color32::from_rgb(0x33, 0x33, 0x33));
                }
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Aa",
                    FontId::proportional(BODY),
                    if resp.hovered() { pal().icon_bright } else { pal().icon },
                );
                if resp.clicked() {
                    self.replace_open = !self.replace_open;
                }

                let delete_clicked = panel_action(ui, false, icon_trash).clicked();
                let redact_clicked = panel_action(ui, false, icon_highlighter).clicked();
                if delete_clicked {
                    self.mark_matches_for_redaction(ctx, true);
                }
                if redact_clicked {
                    self.mark_matches_for_redaction(ctx, false);
                }
            });
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let resp = ui.add_sized(
                    vec2(self.panel_w - 26.0, 28.0),
                    TextEdit::singleline(&mut self.search_query).hint_text("Search text"),
                );
                if std::mem::take(&mut self.find_focus) {
                    resp.request_focus();
                }
                // Une frappe ne cherche pas tout de suite : on attend le repos
                // (`SEARCH_SETTLE_SECS`). L'index, lui, n'est pas reconstruit —
                // seule la correspondance sur le texte déjà plié est refaite.
                if resp.changed() {
                    self.search_typed_at = Some(ui.input(|i| i.time));
                }
                let settle = self.search_typed_at.is_some_and(|at| {
                    ui.input(|i| i.time) - at >= SEARCH_SETTLE_SECS
                });
                if settle {
                    self.search_typed_at = None;
                    let options = SearchOptions {
                        case_sensitive: self.search_case,
                        whole_word: self.search_word,
                    };
                    match self.search_needle() {
                        Some(query) => {
                            self.refresh_search_results(ctx, &query, options, false);
                        }
                        None => self.clear_search_state(),
                    }
                } else if let Some(at) = self.search_typed_at {
                    // Sans réveil programmé, le repos n'arriverait qu'à la
                    // prochaine frame (frappe, clic...).
                    let wait = (SEARCH_SETTLE_SECS as f32) - (ui.input(|i| i.time) - at) as f32;
                    ctx.request_repaint_after_secs(wait.max(0.0));
                }
                // Entrée : egui retire le focus du champ multiligne juste après
                // avoir émis la touche. Le rendre au champ permet d'enchaîner
                // Entrée sur Entrée pour parcourir les résultats.
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // Entrée vaut recherche explicite si la frappe n'est pas
                    // encore partie au repos ; ensuite elle parcourt les
                    // résultats comme ⌘G.
                    let pending = self.search_typed_at.take().is_some();
                    if pending || self.results.is_empty() {
                        self.run_search(ctx);
                    } else {
                        self.search_step(1);
                    }
                    resp.request_focus();
                }
            });

            if self.replace_open {
                ui.add_space(SPACE_M);
                let mut replace_now = false;
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    let resp = ui.add_sized(
                        vec2(self.panel_w - 26.0, 28.0),
                        TextEdit::singleline(&mut self.replace_text).hint_text("Replace with"),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        replace_now = true;
                    }
                });
                ui.add_space(SPACE_M);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    if ui.button(t("Replace all")).clicked() {
                        replace_now = true;
                    }
                });
                if replace_now {
                    let replacement = self.replace_text.clone();
                    let changed = self.replace_matches(ctx, &replacement);
                    if changed > 0 {
                        self.toast(ctx, format!("{changed} replacement(s)"));
                    } else {
                        self.toast(ctx, "Nothing replaced");
                    }
                }
            }
            ui.add_space(12.0);

            let mut jump = None;
            let mut forget = None;
            let mut clear_history = false;
            let mut pending_step: Option<isize> = None;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.results.is_empty() {
                        if self.history.is_empty() {
                            return;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.label(RichText::new(t("History")).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_space(10.0);
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::same(RADIUS_M),
                                        Color32::from_rgb(0x33, 0x33, 0x33),
                                    );
                                }
                                icon_trash(
                                    ui.painter(),
                                    rect.shrink(7.0),
                                    if resp.hovered() { pal().icon_bright } else { pal().icon },
                                );
                                if resp.clicked() {
                                    clear_history = true;
                                }
                            });
                        });
                        ui.add_space(SPACE_M);
                        for (idx, query) in self.history.iter().enumerate() {
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(self.panel_w - 24.0, 28.0), Sense::click());
                            ui.painter()
                                .rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
                            ui.painter().text(
                                pos2(rect.min.x + 10.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                shorten(query, 22),
                                FontId::proportional(BODY),
                                pal().icon_bright,
                            );
                            let x_rect = Rect::from_center_size(
                                pos2(rect.max.x - 14.0, rect.center().y),
                                vec2(16.0, 16.0),
                            );
                            let x_resp =
                                ui.interact(x_rect, Id::new(("hist-x", idx)), Sense::click());
                            icon_close(
                                ui.painter(),
                                x_rect,
                                if x_resp.hovered() { pal().icon_bright } else { pal().muted },
                            );
                            if x_resp.clicked() {
                                forget = Some(idx);
                                continue;
                            }
                            if resp.clicked() {
                                jump = Some(idx);
                            }
                            ui.add_space(SPACE_S);
                        }
                    } else {
                        let active = self.search_active;
                        let total = self.results.len();
                        let mut step: Option<isize> = None;
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            let shown = active.map(|idx| idx + 1).unwrap_or(0);
                            ui.label(
                                RichText::new(format!("Results · {shown}/{total}"))

                                    .strong(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_space(10.0);
                                if panel_action(ui, false, icon_chevron).clicked() {
                                    step = Some(1);
                                }
                                {
                                    // Flèche vers le haut : résultat précédent.
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            Rounding::same(RADIUS_M),
                                            Color32::from_rgb(0x33, 0x33, 0x33),
                                        );
                                    }
                                    icon_caret_up(
                                        ui.painter(),
                                        rect.shrink(8.0),
                                        if resp.hovered() { pal().icon_bright } else { pal().icon },
                                    );
                                    if resp.clicked() {
                                        step = Some(-1);
                                    }
                                }
                            });
                        });
                        if let Some(delta) = step {
                            pending_step = Some(delta);
                        }
                        ui.add_space(SPACE_M);
                        for (idx, result) in self.results.iter().enumerate() {
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(self.panel_w - 24.0, 28.0), Sense::click());
                            if resp.hovered() || active == Some(idx) {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(RADIUS_M),
                                    if active == Some(idx) {
                                        pal().hover_bg
                                    } else {
                                        pal().widget_bg
                                    },
                                );
                            }
                            ui.painter().text(
                                pos2(rect.min.x + 8.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                format!("#{}", result.page),
                                FontId::proportional(BODY),
                                pal().accent,
                            );
                            ui.painter().text(
                                pos2(rect.min.x + 40.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                shorten(result.context.trim(), 24),
                                FontId::proportional(LABEL),
                                pal().muted,
                            );
                            if resp.clicked() {
                                jump = Some(idx);
                            }
                            ui.add_space(SPACE_S);
                        }
                    }
                });

            if clear_history {
                self.history.clear();
            }
            if let Some(idx) = forget {
                self.history.remove(idx);
            }
            if let Some(delta) = pending_step {
                self.search_step(delta);
            }
            if let Some(idx) = jump {
                if self.results.is_empty() {
                    if let Some(query) = self.history.get(idx).cloned() {
                        self.search_query = query;
                        self.run_search(ctx);
                    }
                } else if self.results.get(idx).is_some() {
                    self.search_active = Some(idx);
                    self.focus_search_result();
                }
            }
        }

}
