//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel home_ui).


use super::*;

impl YouPdfApp {

        /// Écran d'accueil : recherche, onglets Récents / Favoris / Outils, et
        /// grille de vignettes ou liste. Remplace l'ancienne liste de récents.
        pub(super) fn home_screen(&mut self, ui: &mut egui::Ui) {
            let mut open: Option<PathBuf> = None;
            let mut open_new: Option<PathBuf> = None;
            let mut reveal: Option<PathBuf> = None;
            let mut forget: Option<PathBuf> = None;
            let mut delete: Option<PathBuf> = None;
            let mut star: Option<PathBuf> = None;
            let mut open_dialog = false;
            let mut clear = false;
            let mut tool: Option<&'static str> = None;

            let now = ui.input(|i| i.time) as i64;

            // Liste affichée selon l'onglet.
            let paths: Vec<PathBuf> = match self.home.category {
                HomeCategory::Recents => self.recents.clone(),
                HomeCategory::Starred => self.stars.clone(),
                HomeCategory::Tools => Vec::new(),
            };
            let visible: Vec<PathBuf> = paths
                .into_iter()
                .filter(|path| {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.home.matches(&name)
                })
                .collect();

            let ctx = ui.ctx().clone();
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // En-tête.
                    ui.add_space(34.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(t("YouPDF")).size(30.0).strong());
                        ui.add_space(SPACE_S);
                        ui.label(
                            RichText::new(t("Free, local-first PDF editing"))
                                .size(BODY)
                                .color(pal().muted),
                        );
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(t("or drop a file anywhere"))
                                .size(LABEL)
                                .color(pal().muted),
                        );
                    });

                    // Barre d'actions : recherche, catégories, vue.
                    let full = ui.available_width();
                    let margin = ((full - 720.0) * 0.5).max(16.0);
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.add_sized(
                            vec2(240.0, 28.0),
                            TextEdit::singleline(&mut self.home.search_filter)
                                .hint_text("Search file…"),
                        );
                        if ui
                            .add_sized(
                                vec2(96.0, 28.0),
                                Button::new(RichText::new(t("Open File")).size(BODY)).fill(pal().accent),
                            )
                            .clicked()
                        {
                            open_dialog = true;
                        }
                        if ui
                            .add_sized(vec2(76.0, 28.0), Button::new(RichText::new(t("Batch")).size(BODY)))
                            .clicked()
                        {
                            tool = Some("batch");
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(margin);
                            let toggle = ui.add_sized(
                                vec2(30.0, 26.0),
                                Button::new(
                                    RichText::new(match self.home.view_mode {
                                        ViewMode::Grid => "▦",
                                        ViewMode::List => "☰",
                                    })
                                    .size(TITLE),
                                ),
                            );
                            let toggle = toggle.on_hover_text(self.home.view_mode.label());
                            if toggle.clicked() {
                                self.home.view_mode = self.home.view_mode.toggled();
                            }
                            ui.add_space(10.0);
                            for category in HomeCategory::ALL.into_iter().rev() {
                                let selected = self.home.category == category;
                                if ui
                                    .selectable_label(selected, RichText::new(category.label()).size(BODY))
                                    .clicked()
                                {
                                    self.home.category = category;
                                }
                                ui.add_space(SPACE_M);
                            }
                        });
                    });
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.label(
                            RichText::new(match self.home.category {
                                HomeCategory::Recents => "Recent files",
                                HomeCategory::Starred => "Starred files",
                                HomeCategory::Tools => "Tools",
                            })
                            .size(TITLE)
                            .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(margin);
                            if self.home.category == HomeCategory::Recents
                                && !self.recents.is_empty()
                                && ui.small_button("Clear").clicked()
                            {
                                clear = true;
                            }
                        });
                    });
                    ui.add_space(10.0);

                    if self.home.category == HomeCategory::Tools {
                        // Raccourcis d'outils.
                        self.home_tools(ui, margin, &mut tool);
                    } else if visible.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(margin);
                            ui.label(
                                RichText::new(if self.home.category == HomeCategory::Starred {
                                    "No starred files yet. Use the star on a card."
                                } else {
                                    "No recent files. Open a PDF to get started."
                                })

                                .color(pal().muted),
                            );
                        });
                    } else if self.home.view_mode == ViewMode::Grid {
                        self.home_grid(
                            ui, margin, &ctx, now, &visible, &mut open, &mut open_new, &mut reveal,
                            &mut forget, &mut delete, &mut star,
                        );
                    } else {
                        self.home_list(
                            ui, margin, now, &visible, &mut open, &mut open_new, &mut reveal,
                            &mut forget, &mut delete, &mut star,
                        );
                    }
                    ui.add_space(30.0);
                });

            if open_dialog {
                self.open_dialog();
            }
            if clear {
                self.recents.clear();
                save_recents(&self.recents);
            }
            if let Some(path) = star {
                home::toggle_star(&mut self.stars, &path);
                home::save_stars(&self.stars);
            }
            if let Some(path) = forget {
                self.recents.retain(|old| old != &path);
                self.home_thumbs.remove(&path);
                save_recents(&self.recents);
            }
            if let Some(path) = reveal {
                self.reveal_path(&path);
            }
            if let Some(path) = delete {
                self.recents.retain(|old| old != &path);
                self.stars.retain(|old| old != &path);
                self.home_thumbs.remove(&path);
                save_recents(&self.recents);
                home::save_stars(&self.stars);
                if std::fs::remove_file(&path).is_ok() {
                    self.toast(&ctx, "File deleted");
                } else {
                    self.toast(&ctx, "Could not delete the file");
                }
            }
            if let Some(path) = open_new {
                if path.is_file() {
                    self.open_path_new_tab(path);
                } else {
                    self.toast(&ctx, "File not found");
                }
                self.home_open = false;
            } else if let Some(path) = open {
                if path.is_file() {
                    self.open_path(path);
                } else {
                    self.toast(&ctx, "File not found");
                }
                self.home_open = false;
            }
            if let Some(id) = tool {
                self.run_home_tool(id);
            }
        }

        /// Une carte de l'écran d'accueil : vignette, nom, taille, ancienneté, étoile.
        #[allow(clippy::too_many_arguments)]
        pub(super) fn home_card(
            &mut self,
            ui: &mut egui::Ui,
            ctx: &egui::Context,
            now: i64,
            path: &std::path::Path,
            open: &mut Option<PathBuf>,
            open_new: &mut Option<PathBuf>,
            reveal: &mut Option<PathBuf>,
            forget: &mut Option<PathBuf>,
            delete: &mut Option<PathBuf>,
            star: &mut Option<PathBuf>,
        ) {
            let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
            }

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let meta = path.metadata().ok();
            let exists = meta.as_ref().is_some_and(|m| m.is_file());
            let starred = home::is_star(&self.stars, path);

            // Vignette de la première page, ou glyphe de repli.
            let thumb_rect = Rect::from_min_size(rect.min, vec2(CARD_W, 130.0));
            let texture = if exists {
                self.home_thumbnail(ctx, path)
            } else {
                None
            };
            match texture {
                Some(texture) => {
                    ui.painter().image(
                        texture.id(),
                        thumb_rect.shrink(6.0),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                None => {
                    let icon = Rect::from_center_size(thumb_rect.center(), vec2(46.0, 58.0));
                    icon_page(ui.painter(), icon, pal().muted);
                }
            }

            // Bas de carte : nom, taille, ancienneté.
            let text_x = rect.min.x + 10.0;
            ui.painter().text(
                pos2(text_x, rect.min.y + 146.0),
                Align2::LEFT_CENTER,
                shorten(&name, 20),
                FontId::proportional(BODY),
                if exists { pal().icon_bright } else { pal().muted },
            );
            ui.painter().text(
                pos2(text_x, rect.min.y + 165.0),
                Align2::LEFT_CENTER,
                if exists {
                    let size = meta
                        .as_ref()
                        .map(|m| format_size(m.len() as usize))
                        .unwrap_or_else(|| "—".to_owned());
                    let age = meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| home::relative_time(d.as_secs() as i64, now))
                        .unwrap_or_else(|| "—".to_owned());
                    format!("{size} · {age}")
                } else {
                    "File not found".to_owned()
                },
                FontId::proportional(CAPTION),
                pal().muted,
            );

            // Étoile.
            let star_rect = Rect::from_center_size(
                pos2(rect.max.x - 16.0, rect.min.y + 148.0),
                vec2(20.0, 20.0),
            );
            let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
            ui.painter().text(
                star_rect.center(),
                Align2::CENTER_CENTER,
                if starred { "★" } else { "☆" },
                FontId::proportional(TITLE),
                if starred { pal().accent } else { pal().muted },
            );
            if star_resp.clicked() {
                *star = Some(path.to_path_buf());
            }

            if star_resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if !star_resp.clicked() && resp.clicked() {
                *open = Some(path.to_path_buf());
            }

            resp.context_menu(|ui| {
                if ui.button(t("Open")).clicked() {
                    *open = Some(path.to_path_buf());
                    ui.close_menu();
                }
                if ui.button(t("Open in New Tab")).clicked() {
                    *open_new = Some(path.to_path_buf());
                    ui.close_menu();
                }
                if ui
                    .button(if starred { "Remove Star" } else { "Star" })
                    .clicked()
                {
                    *star = Some(path.to_path_buf());
                    ui.close_menu();
                }
                if ui.button(t("Reveal in Finder")).clicked() {
                    *reveal = Some(path.to_path_buf());
                    ui.close_menu();
                }
                if ui.button(t("Remove from Recents")).clicked() {
                    *forget = Some(path.to_path_buf());
                    ui.close_menu();
                }
                if ui.button(t("Delete File…")).clicked() {
                    *delete = Some(path.to_path_buf());
                    ui.close_menu();
                }
            });
        }

        /// Liste compacte de l'écran d'accueil.
        #[allow(clippy::too_many_arguments)]
        pub(super) fn home_list(
            &mut self,
            ui: &mut egui::Ui,
            margin: f32,
            now: i64,
            visible: &[PathBuf],
            open: &mut Option<PathBuf>,
            open_new: &mut Option<PathBuf>,
            reveal: &mut Option<PathBuf>,
            forget: &mut Option<PathBuf>,
            delete: &mut Option<PathBuf>,
            star: &mut Option<PathBuf>,
        ) {
            for path in visible {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let parent = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let meta = path.metadata().ok();
                let exists = meta.as_ref().is_some_and(|m| m.is_file());
                let starred = home::is_star(&self.stars, path);

                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    let width = (ui.available_width() - margin).max(200.0);
                    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 14.0, rect.min.y + 15.0),
                        Align2::LEFT_CENTER,
                        shorten(&name, 46),
                        FontId::proportional(BODY),
                        if exists { pal().icon_bright } else { pal().muted },
                    );
                    let detail = if exists {
                        let size = meta
                            .as_ref()
                            .map(|m| format_size(m.len() as usize))
                            .unwrap_or_else(|| "—".to_owned());
                        let age = meta
                            .as_ref()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| home::relative_time(d.as_secs() as i64, now))
                            .unwrap_or_else(|| "—".to_owned());
                        format!("{} · {size} · {age}", shorten(&parent, 60))
                    } else {
                        "File not found".to_owned()
                    };
                    ui.painter().text(
                        pos2(rect.min.x + 14.0, rect.min.y + 32.0),
                        Align2::LEFT_CENTER,
                        detail,
                        FontId::proportional(CAPTION),
                        pal().muted,
                    );

                    let star_rect = Rect::from_center_size(
                        pos2(rect.max.x - 46.0, rect.center().y),
                        vec2(20.0, 20.0),
                    );
                    let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
                    ui.painter().text(
                        star_rect.center(),
                        Align2::CENTER_CENTER,
                        if starred { "★" } else { "☆" },
                        FontId::proportional(TITLE),
                        if starred { pal().accent } else { pal().muted },
                    );
                    if star_resp.clicked() {
                        *star = Some(path.clone());
                    }

                    let forget_rect = Rect::from_center_size(
                        pos2(rect.max.x - 20.0, rect.center().y),
                        vec2(20.0, 20.0),
                    );
                    let forget_resp = ui.interact(forget_rect, resp.id.with("forget"), Sense::click());
                    ui.painter().text(
                        forget_rect.center(),
                        Align2::CENTER_CENTER,
                        if forget_resp.hovered() { ph::X } else { "" },
                        FontId::proportional(BODY),
                        pal().icon,
                    );
                    if forget_resp.clicked() {
                        *forget = Some(path.clone());
                    } else if resp.clicked() {
                        *open = Some(path.clone());
                    }

                    resp.context_menu(|ui| {
                        if ui.button(t("Open")).clicked() {
                            *open = Some(path.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("Open in New Tab")).clicked() {
                            *open_new = Some(path.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("Reveal in Finder")).clicked() {
                            *reveal = Some(path.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("Remove from Recents")).clicked() {
                            *forget = Some(path.clone());
                            ui.close_menu();
                        }
                        if ui.button(t("Delete File…")).clicked() {
                            *delete = Some(path.clone());
                            ui.close_menu();
                        }
                    });
                });
                ui.add_space(SPACE_S);
            }
        }

}
