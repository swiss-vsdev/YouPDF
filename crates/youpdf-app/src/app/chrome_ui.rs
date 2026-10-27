//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel chrome_ui).


use super::*;

impl YouPdfApp {

        pub(super) fn title_bar(&mut self, ctx: &egui::Context) {
            TopBottomPanel::top("titlebar")
                .exact_height(TITLE_H)
                .frame(Frame::none().fill(pal().chrome_bg))
                .show(ctx, |ui| {
                    // Drag fenêtre : interaction posée en premier = priorité basse,
                    // les boutons ajoutés ensuite restent cliquables.
                    let drag = ui.interact(ui.max_rect(), Id::new("titlebar-drag"), Sense::drag());
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }

                    ui.horizontal_centered(|ui| {
                        ui.add_space(76.0); // place des feux de circulation natifs

                        let home = icon_button(ui, 30.0, 28.0, self.home_open, icon_home);
                        if home.clicked() {
                            self.home_open = !self.home_open;
                        }
                        ui.add_space(12.0);

                        let mut select = None;
                        let mut close = None;
                        // Actions différées : la boucle emprunte `self.tabs` en
                        // lecture, elles s'appliquent après.
                        let mut tab_action: Option<TabAction> = None;
                        let mut drag_done = false;
                        let dragging = self.dragging_tab;
                        for (idx, tab) in self.tabs.iter().enumerate() {
                            let selected = idx == self.active;
                            let color = if selected { pal().icon_bright } else { pal().muted };
                            let mut label = if tab.dirty {
                                format!("{} •", shorten(&tab.title, 22))
                            } else {
                                shorten(&tab.title, 24)
                            };
                            if tab.pinned {
                                label = format!("{} {label}", ph::PUSH_PIN);
                            }
                            let galley = ui.painter().layout_no_wrap(
                                label,
                                FontId::proportional(BODY),
                                color,
                            );
                            let width = if selected {
                                (galley.size().x + 56.0).max(170.0)
                            } else {
                                // 44 pt réservent la place du bouton de fermeture,
                                // affiché dès le survol.
                                galley.size().x + 44.0
                            };
                            let width = width.clamp(120.0, 220.0);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());

                            if selected {
                                ui.painter().rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(RADIUS_L),
                                    Color32::from_rgb(0x33, 0x33, 0x33),
                                );
                            }
                            if dragging.is_some() && resp.contains_pointer() {
                                ui.painter().vline(
                                    rect.left() - 5.0,
                                    rect.y_range(),
                                    Stroke::new(2.0_f32, pal().accent),
                                );
                            }
                            ui.painter().galley(
                                pos2(rect.min.x + 14.0, rect.center().y - galley.size().y / 2.0),
                                galley,
                                color,
                            );

                            let show_close = selected || resp.hovered();
                            if show_close {
                                let x_rect = Rect::from_center_size(
                                    pos2(rect.max.x - 16.0, rect.center().y),
                                    vec2(18.0, 18.0),
                                );
                                let x_resp =
                                    ui.interact(x_rect, Id::new(("tab-close", idx)), Sense::click());
                                if x_resp.hovered() {
                                    ui.painter().rect_filled(
                                        x_rect,
                                        Rounding::same(RADIUS_S),
                                        Color32::from_rgb(0x4A, 0x4A, 0x4A),
                                    );
                                }
                                icon_close(
                                    ui.painter(),
                                    x_rect,
                                    if x_resp.hovered() { pal().icon_bright } else { pal().muted },
                                );
                                if x_resp.clicked() {
                                    close = Some(idx);
                                }
                            }

                            if resp.clicked() {
                                select = Some(idx);
                            }
                            if resp.middle_clicked() {
                                close = Some(idx);
                            }
                            if resp.drag_started() && !tab.pinned {
                                self.dragging_tab = Some(idx);
                            }
                            if dragging.is_some() && resp.contains_pointer() {
                                self.drag_target = Some(idx);
                            }
                            if resp.drag_stopped() {
                                drag_done = true;
                            }

                            resp.context_menu(|ui| {
                                if ui.button(t("Close Tab")).clicked() {
                                    tab_action = Some(TabAction::Close(idx));
                                    ui.close_menu();
                                }
                                if ui.button(t("Close Other Tabs")).clicked() {
                                    tab_action = Some(TabAction::CloseOthers(idx));
                                    ui.close_menu();
                                }
                                if ui.button(t("Close Tabs to the Right")).clicked() {
                                    tab_action = Some(TabAction::CloseRight(idx));
                                    ui.close_menu();
                                }
                                if ui.button(t("Close All Tabs")).clicked() {
                                    tab_action = Some(TabAction::CloseAll);
                                    ui.close_menu();
                                }
                                ui.separator();
                                let pin_label = if tab.pinned { "Unpin Tab" } else { "Pin Tab" };
                                if ui.button(pin_label).clicked() {
                                    tab_action = Some(TabAction::TogglePin(idx));
                                    ui.close_menu();
                                }
                                if ui.button(t("Duplicate Tab")).clicked() {
                                    tab_action = Some(TabAction::Duplicate(idx));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button(t("Reveal in Finder")).clicked() {
                                    tab_action = Some(TabAction::Reveal(idx));
                                    ui.close_menu();
                                }
                                if ui.button(t("Copy File Path")).clicked() {
                                    tab_action = Some(TabAction::CopyPath(idx));
                                    ui.close_menu();
                                }
                            });
                            ui.add_space(10.0);
                        }

                        // Dépôt d'un glissé : réordonne si la cible a changé.
                        if drag_done {
                            if let Some(from) = self.dragging_tab.take() {
                                let to = self.drag_target.take().unwrap_or(from);
                                self.move_tab(from, to);
                            }
                        }

                        if let Some(action) = tab_action {
                            self.apply_tab_action(ctx, action);
                        }

                        let plus = icon_button(ui, 24.0, 24.0, false, icon_plus);
                        if plus.clicked() {
                            self.open_dialog();
                        }

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(12.0);
                            // Avatar maison (identité YouPDF).
                            let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                            ui.painter()
                                .circle_filled(rect.center(), 10.5, pal().accent);
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "Y",
                                FontId::proportional(LABEL),
                                Color32::WHITE,
                            );
                            let _ = resp;
                            ui.add_space(SPACE_M);
                            let bell = icon_button(ui, 26.0, 26.0, false, icon_bell);
                            if bell.clicked() {
                                self.activity_open = !self.activity_open;
                            }
                            ui.add_space(4.0);
                            let overflow = icon_button(ui, 24.0, 24.0, false, icon_ellipsis);
                            if overflow.clicked() {
                                self.tools_open = !self.tools_open;
                            }
                        });

                        if let Some(idx) = select {
                            self.active = idx;
                        }
                        if let Some(idx) = close {
                            self.close_tab(idx);
                        }
                    });
                });
        }

        pub(super) fn side_panel(&mut self, ctx: &egui::Context) {
            let Some(panel) = self.panel else {
                return;
            };
            let panel_resp = SidePanel::left("content")
                .default_width(self.panel_w)
                .resizable(true)
                .width_range(180.0..=450.0)
                .frame(Frame::none().fill(pal().canvas_bg).stroke(Stroke::new(1.0_f32, pal().separator)))
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(panel.title()).size(TITLE).strong());
                        if panel == Panel::Thumbnails {
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
                                icon_grid(
                                    ui.painter(),
                                    rect.shrink(7.0),
                                    if resp.hovered() { pal().icon_bright } else { pal().icon },
                                );
                                if resp.clicked() {
                                    self.organize_open = true;
                                }
                            });
                        }
                        if panel == Panel::Search {
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
                                let active = self.search_case
                                    || self.search_word
                                    || self.search_bookmarks
                                    || self.search_comments;
                                icon_funnel(
                                    ui.painter(),
                                    rect.shrink(7.0),
                                    if resp.hovered() || active { pal().icon_bright } else { pal().icon },
                                );
                                if resp.clicked() {
                                    ui.memory_mut(|m| m.toggle_popup(Id::new("search_filters")));
                                }
                                let mut refilter = false;
                                egui::popup_above_or_below_widget(
                                    ui,
                                    Id::new("search_filters"),
                                    &resp,
                                    egui::AboveOrBelow::Below,
                                    egui::PopupCloseBehavior::CloseOnClickOutside,
                                    |ui| {
                                        ui.set_min_width(150.0);
                                        ui.label(
                                            RichText::new(t("Search options"))
                                                .size(LABEL)
                                                .color(pal().muted),
                                        );
                                        ui.add_space(SPACE_S);
                                        refilter |= ui
                                            .checkbox(&mut self.search_case, t("Case sensitive"))
                                            .changed();
                                        refilter |= ui
                                            .checkbox(&mut self.search_word, t("Whole words"))
                                            .changed();
                                        refilter |= ui
                                            .checkbox(&mut self.search_bookmarks, t("Include bookmarks"))
                                            .changed();
                                        refilter |= ui
                                            .checkbox(&mut self.search_comments, t("Include comments"))
                                            .changed();
                                    },
                                );
                                if refilter {
                                    self.run_search(ctx);
                                }
                            });
                        }
                    });
                    ui.add_space(SPACE_L);
                    let sep_y = ui.cursor().top();
                    ui.painter().hline(
                        ui.max_rect().x_range(),
                        sep_y,
                        Stroke::new(1.0_f32, pal().separator),
                    );
                    ui.add_space(10.0);
                    match panel {
                        Panel::Thumbnails => self.thumbnails(ui),
                        Panel::Search => self.search_ui(ctx, ui),
                        Panel::Comments => self.comments_panel(ctx, ui),
                        Panel::Bookmarks => self.bookmarks_panel(ctx, ui),
                        Panel::Forms => self.forms_panel(ctx, ui),
                        Panel::Attachments => self.attachments_panel(ctx, ui),
                        Panel::Aspect => self.aspect_panel(ui),
                    }
                });
            // La largeur réelle suit le glissé ; les panneaux s'y ajustent à la
            // trame suivante.
            self.panel_w = panel_resp.response.rect.width();
        }

        /// Pastille bas-droite : page · zoom, façon « organize » de la référence.
        /// Barre d'état inférieure : navigation de page, disposition, zoom,
        /// diaporama, dimensions de page et état de protection.
        ///
        /// Remplace l'ancienne barre de zoom flottante : un seul endroit porte les
        /// réglages de vue, comme dans UPDF.
        pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
            if self.home_open || self.tabs.is_empty() {
                return;
            }
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let page = tab.page;
            let total = tab.total();
            let zoom_fit_page = tab.zoom_fit_page;
            let current_zoom = tab.zoom;
            let zoom_label = current_zoom
                .map(|z| format!("{:.0}%", z * 100.0))
                .unwrap_or_else(|| {
                    if zoom_fit_page {
                        "Fit page".to_owned()
                    } else {
                        "Fit width".to_owned()
                    }
                });
            let layout = self.page_layout;
            let protected = tab.protected;
            let engine = self.renderer_label;
            let view = tab.doc.page_view(page as u32);
            let (view_w, view_h) = view.display_size();
            let size_label = page_size_label(view_w, view_h);

            let mut goto: Option<usize> = None;
            let mut delta = 0.0_f32;
            let mut layout_choice: Option<PageLayout> = None;
            let mut zoom_choice: Option<ZoomMode> = None;
            let mut slideshow = false;
            let mut readaloud = false;

            TopBottomPanel::bottom("statusbar")
                .exact_height(STATUS_H)
                .frame(Frame::none().fill(pal().chrome_bg))
                .show(ctx, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(SPACE_M);

                        // Navigation de page.
                        if status_button(ui, ph::CARET_DOUBLE_LEFT, "First page") {
                            goto = Some(1);
                        }
                        if status_button(ui, ph::CARET_LEFT, "Previous page") && page > 1 {
                            goto = Some(page - 1);
                        }
                        // Le champ reprend la page courante tant qu'il n'a pas le
                        // focus ; sinon une saisie en cours serait écrasée.
                        let page_field = Id::new("status-page-entry");
                        if !ui.memory(|m| m.has_focus(page_field)) {
                            self.page_entry = page.to_string();
                        }
                        let field = ui.add_sized(
                            vec2(34.0, 18.0),
                            TextEdit::singleline(&mut self.page_entry)
                                .id(page_field)
                                .hint_text("1"),
                        );
                        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            goto = self.page_entry.trim().parse::<usize>().ok();
                        }
                        ui.label(
                            RichText::new(format!("of {total}"))
                                .size(LABEL)
                                .color(pal().muted),
                        );
                        if status_button(ui, ph::CARET_RIGHT, "Next page") && page < total {
                            goto = Some(page + 1);
                        }
                        if status_button(ui, ph::CARET_DOUBLE_RIGHT, "Last page") {
                            goto = Some(total);
                        }

                        ui.add_space(SPACE_M);
                        ui.separator();
                        ui.add_space(SPACE_M);

                        // Disposition des pages.
                        let layout_resp = ui
                            .add(
                                Button::new(
                                    RichText::new(ph::SQUARES_FOUR).size(TITLE).color(pal().icon),
                                )
                                .frame(false),
                            )
                            .on_hover_text(format!("Layout: {}", layout.label()));
                        if layout_resp.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(Id::new("layout_menu")));
                        }
                        egui::popup_above_or_below_widget(
                            ui,
                            Id::new("layout_menu"),
                            &layout_resp,
                            egui::AboveOrBelow::Above,
                            egui::PopupCloseBehavior::CloseOnClick,
                            |ui| {
                                ui.set_min_width(170.0);
                                for choice in PageLayout::ALL {
                                    let mark = if choice == layout { ph::CHECK } else { "" };
                                    if menu_row(ui, choice.label(), mark, None) {
                                        layout_choice = Some(choice);
                                    }
                                }
                            },
                        );

                        ui.add_space(SPACE_M);
                        ui.separator();
                        ui.add_space(SPACE_M);

                        // Zoom.
                        if status_button(ui, ph::MINUS, "Zoom out") {
                            delta = -1.0;
                        }
                        let text = if zoom_fit_page {
                            format!("{zoom_label} · page")
                        } else {
                            zoom_label.clone()
                        };
                        let zoom_resp = ui
                            .add(
                                Button::new(muted(&text, LABEL))
                                    .frame(false),
                            )
                            .on_hover_text(t("Zoom presets"));
                        if zoom_resp.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(Id::new("zoom_menu")));
                        }
                        egui::popup_above_or_below_widget(
                            ui,
                            Id::new("zoom_menu"),
                            &zoom_resp,
                            egui::AboveOrBelow::Above,
                            egui::PopupCloseBehavior::CloseOnClick,
                            |ui| {
                                ui.set_min_width(150.0);
                                for (label, mode) in ZoomMode::CHOICES {
                                    let selected =
                                        zoom_choice_matches(*mode, current_zoom, zoom_fit_page);
                                    if menu_row(ui, label, if selected { ph::CHECK } else { "" }, None) {
                                        zoom_choice = Some(*mode);
                                    }
                                }
                            },
                        );
                        if status_button(ui, ph::PLUS, "Zoom in") {
                            delta = 1.0;
                        }

                        ui.add_space(SPACE_M);
                        ui.separator();
                        ui.add_space(SPACE_M);

                        if status_button(ui, ph::PLAY, "Slideshow (F5)") {
                            slideshow = true;
                        }

                        if status_button(ui, ph::SPEAKER_SIMPLE_HIGH, "Read Aloud") {
                            readaloud = true;
                        }

                        // Graphe de droite : dimensions et protection.
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(SPACE_M);
                            let (lock_glyph, lock_tip, lock_color) = if protected {
                                (ph::LOCK_SIMPLE, "Protected document", pal().accent)
                            } else {
                                (ph::LOCK_OPEN, "No restrictions", pal().muted)
                            };
                            ui.add(
                                Button::new(RichText::new(lock_glyph).color(lock_color))
                                    .frame(false),
                            )
                            .on_hover_text(lock_tip);
                            ui.add_space(SPACE_S);
                            ui.label(muted(size_label, LABEL));
                            ui.add_space(SPACE_M);
                            ui.label(caption(engine));
                        });
                    });
                });

            if let Some(target) = goto {
                if (1..=total).contains(&target) {
                    self.jump = Some(target);
                } else {
                    self.toast(ctx, format!("Page out of range (1–{total})"));
                }
            }
            if let Some(choice) = layout_choice {
                self.set_page_layout(ctx, choice);
            }
            if let Some(choice) = zoom_choice {
                self.set_zoom_mode(choice);
            }
            if delta < 0.0 {
                self.zoom_by(1.0 / 1.1);
            } else if delta > 0.0 {
                self.zoom_by(1.1);
            }
            if slideshow {
                self.enter_slideshow(ctx);
            }
            if readaloud {
                self.readaloud_open = true;
            }
        }

}
