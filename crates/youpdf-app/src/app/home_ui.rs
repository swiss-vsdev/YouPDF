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
            let thumb_h = CARD_W * 1.414; // A4 aspect ratio for thumbnails
            let thumb_rect = Rect::from_min_size(rect.min, vec2(CARD_W, thumb_h));
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

    /// Fenêtre « Open Quickly » (⌘⇧O) : recherche par sous-suite des récents,
    /// façon Spotlight — champ en haut-centre, douze lignes, Entrée ouvre le
    /// premier, Échap ferme.
    pub(super) fn open_quickly_panel(&mut self, ctx: &egui::Context) {
        egui::Window::new("Open Quickly")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 100.0])
            .show(ctx, |ui| {
                ui.add(TextEdit::singleline(&mut self.open_query).desired_width(400.0))
                    .request_focus();

                // On garde les indices, pas les chemins : `recents` doit rester
                // libre pour l'ouverture.
                let mut matches: Vec<(usize, usize)> = self
                    .recents
                    .iter()
                    .enumerate()
                    .filter_map(|(index, path)| {
                        let score = subsequence_score(&path.display().to_string(), &self.open_query);
                        if score > 0 {
                            Some((index, score))
                        } else {
                            None
                        }
                    })
                    .collect();
                // Tri stable décroissant : à score égal, l'ordre des récents
                // (du plus récent au plus ancien) est conservé.
                matches.sort_by_key(|a| std::cmp::Reverse(a.1));

                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

                for (rank, (index, _score)) in matches.iter().take(12).enumerate() {
                    let label = self.recents[*index].display().to_string();
                    let clicked = ui.selectable_label(false, label).clicked();
                    if clicked || (enter && rank == 0) {
                        let path = self.recents[*index].clone();
                        self.open_path(path);
                        self.open_quickly_open = false;
                        break;
                    }
                }
                if matches.is_empty() {
                    ui.label(RichText::new("No match").color(pal().muted));
                }
                if escape {
                    self.open_quickly_open = false;
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

// Suite du panel home_ui (extraite de `app.rs`).


impl YouPdfApp {

        /// Grille de cartes de l'écran d'accueil (onglets Récents / Favoris).
        #[allow(clippy::too_many_arguments)]
        pub(super) fn home_grid(
            &mut self,
            ui: &mut egui::Ui,
            margin: f32,
            ctx: &egui::Context,
            now: i64,
            visible: &[PathBuf],
            open: &mut Option<PathBuf>,
            open_new: &mut Option<PathBuf>,
            reveal: &mut Option<PathBuf>,
            forget: &mut Option<PathBuf>,
            delete: &mut Option<PathBuf>,
            star: &mut Option<PathBuf>,
        ) {
            let usable = (ui.available_width() - 2.0 * margin).max(CARD_W);
            let columns = ((usable + GAP) / (CARD_W + GAP)).floor().max(1.0) as usize;
            let mut index = 0usize;
            while index < visible.len() {
                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    for path in visible.iter().skip(index).take(columns) {
                        self.home_card(ui, ctx, now, path, open, open_new, reveal, forget, delete, star);
                        ui.add_space(GAP);
                    }
                });
                index += columns;
                ui.add_space(GAP);
            }
        }

        /// Grille de raccourcis d'outils de l'onglet « Tools ».
        pub(super) fn home_tools(&mut self, ui: &mut egui::Ui, margin: f32, tool: &mut Option<&'static str>) {
            const TOOLS: [(&str, &str); 12] = [
                ("Search", "search"),
                ("Thumbnails", "thumbs"),
                ("Organize", "organize"),
                ("Watermark", "watermark"),
                ("Compress", "compress"),
                ("Protect", "protect"),
                ("Crop", "crop"),
                ("OCR", "ocr"),
                ("Compare", "compare"),
                ("Measure", "measure"),
                ("Extract data", "extract"),
                ("Batch", "batch"),
            ];
            let usable = (ui.available_width() - 2.0 * margin).max(CELL_W);
            let columns = ((usable + GAP) / (CELL_W + GAP)).floor().max(1.0) as usize;
            let mut index = 0usize;
            while index < TOOLS.len() {
                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    for (label, id) in TOOLS.iter().skip(index).take(columns) {
                        let (rect, resp) = ui.allocate_exact_size(vec2(CELL_W, CELL_H), Sense::click());
                        if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
                        }
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            *label,
                            FontId::proportional(TITLE),
                            pal().icon_bright,
                        );
                        if resp.clicked() {
                            *tool = Some(id);
                        }
                        ui.add_space(GAP);
                    }
                });
                index += columns;
                ui.add_space(GAP);
            }
        }

        /// Applique un raccourci d'outil choisi dans l'onglet « Tools ».
        pub(super) fn run_home_tool(&mut self, id: &str) {
            match id {
                "search" => self.panel = Some(Panel::Search),
                "thumbs" => self.panel = Some(Panel::Thumbnails),
                "organize" => self.organize_open = true,
                "watermark" => self.watermark_open = true,
                "compress" => self.compress_open = true,
                "protect" => self.protect_open = true,
                "crop" => self.crop_open = true,
                "ocr" => self.ocr_open = true,
                "compare" => self.compare_open = true,
                "measure" => self.measure_open = true,
                "extract" => self.extract_open = true,
                "batch" => self.batch_open = true,
                _ => {}
            }
        }

        /// Vignette de la première page d'un fichier, mise en cache par `mtime`.
        ///
        /// Renvoie `None` pour un fichier illisible, chiffré, ou sans pdfium.
        ///
        /// ponytail: l'ouverture et le rendu se font sur le fil de l'interface.
        /// Acceptable pour une poignée de récents ; déporter sur un fil si la
        /// liste s'allonge ou si des fichiers lourds font scintiller l'accueil.
                pub(super) fn home_thumbnail(&mut self, ctx: &egui::Context, path: &std::path::Path) -> Option<TextureHandle> {
            let meta = std::fs::metadata(path).ok()?;
            // Un fichier énorme n'entre pas dans une vignette : on l'ignore plutôt
            // que de le charger en entier au premier affichage.
            if meta.len() > 64 * 1024 * 1024 {
                return None;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)?;
            if let Some((cached, texture)) = self.home_thumbs.get(path) {
                if *cached == mtime {
                    return Some(texture.clone());
                }
            }
            if !self.renderer.renders_real_content() {
                return None;
            }
            let doc = Document::open(path).ok()?;
            if doc.page_count() == 0 {
                return None;
            }
            let view = doc.page_view(1);
            let scale = (160.0 / view.width.max(1.0)).clamp(0.05, 0.5);
            let options = RenderOptions {
                scale,
                ..Default::default()
            };
            let bitmap = self.renderer.render_page(&doc, 1, &options).ok()?;
            let image = bitmap.to_egui_image();
            // Même fichier, contenu changé : on réécrit la texture existante
            // plutôt que d'en allouer une seconde puis de jeter la première.
            let handle = match self.home_thumbs.get(path) {
                Some((_, texture)) => {
                    let mut texture = texture.clone();
                    texture.set(image, TextureOptions::LINEAR);
                    texture
                }
                None => ctx.load_texture(
                    format!("home-thumb-{}", path.display()),
                    image,
                    TextureOptions::LINEAR,
                ),
            };
            // ponytail: borne simple — vider le cache s'il déborde. Un accueil
            // affiche une poignée de vignettes ; au-delà, les re-rendre coûte
            // moins qu'un cache qui grossit sans fin.
            if self.home_thumbs.len() > 128 {
                self.home_thumbs.clear();
            }
            self.home_thumbs
                .insert(path.to_path_buf(), (mtime, handle.clone()));
            Some(handle)
        }

}

/// Score de correspondance par sous-suite : combien de caractères de `needle`
/// se retrouvent dans `haystack`, dans l'ordre. Renvoie 0 dès qu'un caractère
/// manque — « pas de correspondance » et score 0 se confondent ici, ce qui
/// suffit à un filtre.
///
/// Comparaison insensible à la casse. Le score brut (nombre de caractères
/// trouvés) suffit à classer une liste de récents : tout le monde ne cherche
/// qu'à retrouver un nom de fichier déjà ouvert.
///
/// ponytail: pas de crate `fuzzy-matcher`, pas d'index — une recherche
/// sous-suite en O(n·m) sur une liste de ~50 entrées. Ajouter un index à
/// trigrammes si les récents dépassent 500.
fn subsequence_score(haystack: &str, needle: &str) -> usize {
    let needle = &needle.to_lowercase();
    if needle.is_empty() {
        return 0;
    }
    let haystack = haystack.to_lowercase();
    let mut hp = 0usize;
    let hbytes = haystack.as_bytes();
    for wanted in needle.bytes() {
        let mut found = false;
        while hp < hbytes.len() {
            if hbytes[hp] == wanted {
                hp += 1;
                found = true;
                break;
            }
            hp += 1;
        }
        if !found {
            return 0;
        }
    }
    needle.len()
}

#[cfg(test)]
mod tests {
    use super::subsequence_score;

    #[test]
    fn sous_suite_scoree_et_triee() {
        // La requête se lit dans le chemin, dans l'ordre, sans casse.
        assert_eq!(subsequence_score("/tmp/Rapport Annuel.pdf", "rapport"), 7);
        assert_eq!(subsequence_score("/tmp/a.pdf", "a"), 1);
        // Un caractère manquant élimine : rien ne mérite d'être affiché.
        assert_eq!(subsequence_score("/tmp/a.pdf", "zzz"), 0);
        // Ordre respecté : « za » n'existe pas dans « az ».
        assert_eq!(subsequence_score("/tmp/az.pdf", "za"), 0);
        // Requête vide : rien à filtrer, on n'affiche personne.
        assert_eq!(subsequence_score("/tmp/a.pdf", ""), 0);
        // Le meilleur score passe en tête d'un tri décroissant.
        let paths = ["/tmp/bilan-2024.pdf", "/tmp/rapport-annuel-2024.pdf"];
        let mut ranked: Vec<_> = paths
            .iter()
            .filter_map(|p| {
                let score = subsequence_score(p, "rap2024");
                (score > 0).then_some((p, score))
            })
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1));
        assert_eq!(ranked.first().map(|(p, _)| *p), Some(&"/tmp/rapport-annuel-2024.pdf"));
    }
}
