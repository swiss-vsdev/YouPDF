//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel home_ui).


use super::*;

/// Blanc réservé entre le bord de la fenêtre et la grille, de chaque côté.
const SIDE_GAP: f32 = 34.0;

/// Colonnes et marge gauche d'une grille de `count` cartes, avec le même
/// blanc à gauche et à droite (bord de fenêtre inclus).
fn grid_layout(full: f32, count: usize) -> (usize, f32) {
    let usable = (full - 2.0 * SIDE_GAP).max(CARD_W);
    let columns = ((usable + GAP) / (CARD_W + GAP)).floor().max(1.0) as usize;
    let full_row_w = columns as f32 * CARD_W + (columns - 1) as f32 * GAP;
    let gap = SIDE_GAP + ((usable - full_row_w) * 0.5).max(0.0);
    let _ = count;
    (columns, gap)
}

/// Texte « gras » sans police dédiée : deux passages décalés d'un demi-pixel.
fn bold_text(p: &egui::Painter, pos: egui::Pos2, anchor: egui::Align2, text: &str, size: f32, color: Color32) {
    let font = FontId::proportional(size);
    p.text(pos, anchor, text, font.clone(), color);
    p.text(pos + vec2(0.6, 0.0), anchor, text, font, color);
}

/// Dessine une icône 3D Fluent si disponible, sinon le glyphe Phosphor.
fn paint_icon(
    ui: &mut egui::Ui,
    rect: Rect,
    texture: Option<egui::TextureId>,
    fallback: IconFn,
    color: Color32,
) {
    match texture {
        Some(id) => {
            ui.painter().image(
                id,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => fallback(ui.painter(), rect, color),
    }
}

/// Vignette légèrement « croquée » au survol : vraie rotation du quad
/// texturé (egui n'a pas de `Painter::image` avec angle).
fn paint_tilted(p: &egui::Painter, texture: egui::TextureId, rect: Rect, angle: f32) {
    let center = rect.center();
    let (sin, cos) = angle.sin_cos();
    let rotate = |p: egui::Pos2| {
        let d = p - center;
        pos2(center.x + d.x * cos - d.y * sin, center.y + d.x * sin + d.y * cos)
    };
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let uvs = [
        pos2(0.0, 0.0),
        pos2(1.0, 0.0),
        pos2(1.0, 1.0),
        pos2(0.0, 1.0),
    ];
    let mut mesh = egui::Mesh::with_texture(texture);
    for (corner, uv) in corners.into_iter().zip(uvs) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: rotate(corner),
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    p.add(egui::Shape::mesh(mesh));
}

/// Bouton d'accent UPDF : icône 3D / Phosphor + libellé, fond accent.
fn accent_button(
    ui: &mut egui::Ui,
    texture: Option<egui::TextureId>,
    icon: IconFn,
    label: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(207.0, 34.0), Sense::click());
    let fill = if resp.hovered() {
        Color32::from_rgb(0x8F, 0x93, 0xFF)
    } else {
        pal().accent
    };
    ui.painter().rect_filled(rect, Rounding::same(8.0), fill);
    paint_icon(
        ui,
        Rect::from_center_size(pos2(rect.min.x + 28.0, rect.center().y), vec2(22.0, 22.0)),
        texture,
        icon,
        Color32::WHITE,
    );
    bold_text(
        ui.painter(),
        pos2(rect.min.x + 48.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        15.0,
        Color32::WHITE,
    );
    resp.clicked()
}

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
            let mut tool: Option<&'static str> = None;

            // `ui.input().time` est le temps écoulé depuis le lancement, pas
            // l'horloge murale : le passer à relative_time donnait toujours
            // « Just now ». On veut l'epoch Unix.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

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
                    // En-tête UPDF : pastilles de catégorie à gauche, deux
                    // boutons d'action à droite, séparateur, puis filtres.
                    // Mesuré sur la capture home 2.5.6 (Retina 2×) : pastilles
                    // y≈136–180 px (68–90 pt), actions 132×30 pt, séparateur
                    // y≈222 px (111 pt), premières cartes y≈322 px (161 pt).
                    // Mesures natives UPDF : pastilles y≈208–275 px (104–137 pt),
                    // filtres y≈385–439 px (192–219 pt), cartes dès y≈518 px (259 pt).
                    ui.add_space(30.0);
                    let full = ui.available_width();
                    let (_cols, margin) = grid_layout(full, visible.len());
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        // L'accueil UPDF n'expose que Récent / Favoris ;
                        // l'onglet Outils reste accessible depuis le dock.
                        for category in [HomeCategory::Recents, HomeCategory::Starred] {
                            let selected = self.home.category == category;
                            let btn = ui.add_sized(
                                vec2(98.0, 34.0),
                                Button::new(
                                    RichText::new(t(category.label())).size(TITLE).strong().color(if selected {
                                        pal().icon_bright
                                    } else {
                                        pal().muted
                                    }),
                                )
                                .fill(if selected {
                                    pal().widget_bg
                                } else {
                                    Color32::TRANSPARENT
                                })
                                .rounding(Rounding::same(RADIUS_XL)),
                            );
                            if btn.clicked() {
                                self.home.category = category;
                            }
                            ui.add_space(8.0);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(margin);
                            if accent_button(
                                ui,
                                self.fluent.id(crate::fluent::Icon::Folder),
                                icon_folder_plus,
                                t("Open a file"),
                            ) {
                                open_dialog = true;
                            }
                        });
                    });
                    ui.add_space(26.0);
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.separator();
                    });
                    ui.add_space(28.0);

                    // Ligne de filtres : icônes de tri / recherche / vue à droite.
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(margin);
                            // Ordre inverse : le rendu final est search, sort,
                            // grille, liste — comme dans UPDF. Icônes Phosphor
                            // (les glyphes Unicode « ▦/☰ » rendaient des carrés vides).
                            for (mode, draw) in [
                                (ViewMode::List, icon_list as IconFn),
                                (ViewMode::Grid, icon_grid as IconFn),
                            ] {
                                let active = self.home.view_mode == mode;
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                                if active || resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::same(6.0),
                                        if active {
                                            pal().widget_bg
                                        } else {
                                            pal().hover_bg
                                        },
                                    );
                                }
                                let color = if active || resp.hovered() {
                                    pal().icon_bright
                                } else {
                                    pal().muted
                                };
                                draw(ui.painter(), rect.shrink(5.0), color);
                                if resp.on_hover_text(mode.label()).clicked() {
                                    self.home.view_mode = mode;
                                }
                                ui.add_space(2.0);
                            }
                            ui.add_space(10.0);
                            let (sort_rect, sort) =
                                ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                            icon_sort(ui.painter(), sort_rect, pal().muted);
                            let _ = sort.on_hover_text(t("Sort"));
                            ui.add_space(10.0);
                            if self.home.search_open {
                                ui.add_sized(
                                    vec2(240.0, 30.0),
                                    TextEdit::singleline(&mut self.home.search_filter)
                                        .hint_text(t("Search file…")),
                                );
                            }
                            let (search_rect, search) =
                                ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                            icon_search(ui.painter(), search_rect, pal().muted);
                            if search.on_hover_text(t("Search file…")).clicked() {
                                self.home.search_open = !self.home.search_open;
                            }
                        });
                    });
                    ui.add_space(22.0);

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
                    ui.add_space(90.0);
                });

            // Dock flottant UPDF : Outils / IA, centré en bas.
            // Mesuré sur la capture home 2.5.6 : ≈ 180×45 pt, 18 pt du bas.
            egui::Area::new(egui::Id::new("home-dock"))
                .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -18.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::none()
                        .fill(pal().widget_bg)
                        .rounding(Rounding::same(RADIUS_3XL))
                        .inner_margin(Margin::symmetric(14.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let dock_button = |ui: &mut egui::Ui,
                                                   texture: Option<egui::TextureId>,
                                                   draw: IconFn,
                                                   label: &str|
                                 -> egui::Response {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(100.0, 60.0), Sense::click());
                                    let color = if resp.hovered() {
                                        pal().icon_bright
                                    } else {
                                        pal().icon
                                    };
                                    paint_icon(
                                        ui,
                                        Rect::from_center_size(
                                            rect.center_top() + vec2(0.0, 16.0),
                                            vec2(30.0, 30.0),
                                        ),
                                        texture,
                                        draw,
                                        color,
                                    );
                                    bold_text(
                                        ui.painter(),
                                        rect.center_bottom() + vec2(0.0, -12.0),
                                        egui::Align2::CENTER_CENTER,
                                        label,
                                        14.0,
                                        color,
                                    );
                                    resp
                                };
                                if dock_button(
                                    ui,
                                    self.fluent.id(crate::fluent::Icon::Gear),
                                    icon_tools,
                                    t("Tools"),
                                )
                                .clicked()
                                {
                                    self.home.category = HomeCategory::Tools;
                                }
                                if dock_button(
                                    ui,
                                    self.fluent.id(crate::fluent::Icon::Magic),
                                    icon_sparkle,
                                    "IA",
                                )
                                .clicked()
                                {
                                    self.ai.open = true;
                                }
                            });
                        });
                });

            if open_dialog {
                self.open_dialog();
            }
            if let Some(path) = star {
                home::toggle_star(&mut self.stars, &path);
                home::save_stars(&self.stars);
            }
            if let Some(path) = forget {
                self.recents.retain(|old| old != &path);
                self.home_thumbs.remove(&path);
                self.persist_recents();
            }
            if let Some(path) = reveal {
                self.reveal_path(&path);
            }
            if let Some(path) = delete {
                // Suppression définitive : confirmation avant tout `remove_file`.
                self.home.pending_delete = Some(path);
            }
            self.confirm_delete_file(&ctx);
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

        /// Confirmation d'une suppression définitive demandée depuis l'accueil.
        pub(super) fn confirm_delete_file(&mut self, ctx: &egui::Context) {
            let Some(path) = self.home.pending_delete.clone() else {
                return;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let mut open = true;
            let mut confirmed = false;
            let mut cancel = false;
            self.show_modal(
                ctx,
                "confirm-delete-file",
                "Delete file",
                420.0,
                &mut open,
                |_app, ui| {
                    ui.label(RichText::new(format!("Delete “{name}” from your computer?")));
                    ui.add_space(SPACE_S);
                    ui.label(
                        RichText::new("This permanently deletes the file. It cannot be undone.")
                            .color(pal().state_bad),
                    );
                    ui.add_space(SPACE_M);
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() {
                            cancel = true;
                        }
                        confirmed = ui
                            .button(RichText::new("Delete file").color(pal().state_bad))
                            .clicked();
                    });
                },
            );
            if cancel {
                open = false;
            }
            if !open {
                self.home.pending_delete = None;
            }
            if confirmed {
                // Les entrées ne sont retirées qu'après une suppression réussie.
                if std::fs::remove_file(&path).is_ok() {
                    self.recents.retain(|old| old != &path);
                    self.stars.retain(|old| old != &path);
                    self.home_thumbs.remove(&path);
                    self.persist_recents();
                    home::save_stars(&self.stars);
                    self.home.pending_delete = None;
                    self.toast(ctx, "File deleted");
                } else {
                    self.toast(ctx, "Could not delete the file — check its permissions");
                }
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
            // Fond de carte UPDF : présent au repos, plus clair au survol.
            ui.painter().rect_filled(
                rect,
                Rounding::same(RADIUS_XL),
                if resp.hovered() {
                    pal().hover_bg
                } else {
                    pal().widget_bg
                },
            );

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let meta = path.metadata().ok();
            let exists = meta.as_ref().is_some_and(|m| m.is_file());
            let starred = home::is_star(&self.stars, path);

            // Haut de carte : nom puis métadonnées (ordre de la capture UPDF).
            let text_x = rect.min.x + 18.0;
            let meta_text = if exists {
                let size = meta
                    .as_ref()
                    .map(|m| format_size(m.len() as usize))
                    .unwrap_or_else(|| "—".to_owned());
                // Dernière *ouverture*, pas date de modification du fichier.
                let opened = self.recent_times.get(path).copied().or_else(|| {
                    meta.as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                });
                let age = opened
                    .map(|t| home::relative_time(t as i64, now))
                    .unwrap_or_else(|| "—".to_owned());
                format!("{size} · {age}")
            } else {
                "File not found".to_owned()
            };
            ui.painter().text(
                pos2(text_x, rect.min.y + 43.0),
                Align2::LEFT_CENTER,
                shorten(&name, 22),
                FontId::proportional(14.0),
                if exists { pal().icon_bright } else { pal().muted },
            );
            ui.painter().text(
                pos2(text_x, rect.min.y + 80.0),
                Align2::LEFT_CENTER,
                meta_text,
                FontId::proportional(11.0),
                pal().muted,
            );

            // Vignette sous le texte, dans le bas de la carte.
            let thumb_rect = Rect::from_min_max(
                pos2(rect.min.x + 18.0, rect.min.y + 102.0),
                pos2(rect.max.x - 10.0, rect.max.y - 6.0),
            );
            let texture = if exists {
                self.home_thumbnail(ctx, path)
            } else {
                None
            };
            match texture {
                Some(texture) => {
                    // Remplissage « cover » : la page est coupée, comme dans UPDF.
                    let fitted = fitted_thumb(thumb_rect, texture.size());
                    // Au survol, on élargit le cadre de découpe à gauche et
                    // en haut : la page peut bouger en entier pendant la
                    // rotation sans être tronquée, sans bordure visible.
                    let clip = if resp.hovered() {
                        Rect::from_min_max(
                            pos2(thumb_rect.min.x - 34.0, thumb_rect.min.y - 34.0),
                            pos2(thumb_rect.max.x + 6.0, thumb_rect.max.y + 6.0),
                        )
                    } else {
                        thumb_rect
                    };
                    let painter = ui.painter().with_clip_rect(clip);
                    if resp.hovered() {
                        // Léger effet « croqué » (capture 18.58.42) : rotation
                        // douce du quad texturé, bords coupés par le clip.
                        paint_tilted(&painter, texture.id(), fitted, 3.0_f32.to_radians());
                    } else {
                        painter.image(
                            texture.id(),
                            fitted,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                }
                None => {
                    let icon = Rect::from_center_size(thumb_rect.center(), vec2(46.0, 58.0));
                    icon_page(ui.painter(), icon, pal().muted);
                }
            }
            // Étoile.
            let star_rect = Rect::from_center_size(
                pos2(rect.max.x - 22.0, rect.min.y + 43.0),
                vec2(20.0, 20.0),
            );
            let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
            if starred || resp.hovered() {
                ui.painter().text(
                    star_rect.center(),
                    Align2::CENTER_CENTER,
                    if starred { "★" } else { "☆" },
                    FontId::proportional(TITLE),
                    if starred { pal().accent } else { pal().muted },
                );
            }
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
        Popup::window(t("Open Quickly"))
            .fixed()
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 100.0))
            .show_detached(ctx, |ui| {
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
            _margin: f32,
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
            // Une seule marge calculée sur une rangée pleine : toutes les
            // rangées commencent au même x, avec le même blanc à droite.
            let (columns, left_gap) = grid_layout(ui.available_width(), visible.len());
            let mut index = 0usize;
            while index < visible.len() {
                let row: Vec<_> = visible.iter().skip(index).take(columns).collect();
                ui.horizontal(|ui| {
                    // egui ajoute `item_spacing.x` (6 pt) en plus de GAP.
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(left_gap);
                    for (i, path) in row.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(GAP);
                        }
                        self.home_card(ui, ctx, now, path, open, open_new, reveal, forget, delete, star);
                    }
                });
                index += columns;
                ui.add_space(GAP);
            }
        }

        /// Grille de raccourcis d'outils de l'onglet « Tools ».
        pub(super) fn home_tools(&mut self, ui: &mut egui::Ui, _margin: f32, tool: &mut Option<&'static str>) {
            // Tuiles avec vraies icônes Phosphor, comme le panneau Outils.
            const TOOLS: [(&str, &str, IconFn); 12] = [
                ("Search", "search", icon_search),
                ("Thumbnails", "thumbs", icon_page),
                ("Organize", "organize", icon_organize),
                ("Watermark", "watermark", icon_text),
                ("Compress", "compress", icon_compress),
                ("Protect", "protect", icon_lock),
                ("Crop", "crop", icon_scissors),
                ("OCR", "ocr", icon_text),
                ("Compare", "compare", icon_search),
                ("Measure", "measure", icon_measure),
                ("Extract data", "extract", icon_export),
                ("Batch", "batch", icon_grid),
            ];
            let (columns, left_gap) = grid_layout(ui.available_width(), TOOLS.len());
            let mut index = 0usize;
            while index < TOOLS.len() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(left_gap);
                    for (i, (label, id, draw)) in TOOLS
                        .iter()
                        .skip(index)
                        .take(columns)
                        .enumerate()
                    {
                        if i > 0 {
                            ui.add_space(GAP);
                        }
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(CARD_W, 116.0), Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            Rounding::same(RADIUS_XL),
                            if resp.hovered() {
                                pal().hover_bg
                            } else {
                                pal().widget_bg
                            },
                        );
                        let color = pal().icon_bright;
                        draw(
                            ui.painter(),
                            Rect::from_center_size(rect.center() - vec2(0.0, 14.0), vec2(34.0, 34.0)),
                            color,
                        );
                        bold_text(
                            ui.painter(),
                            rect.center_bottom() - vec2(0.0, 22.0),
                            Align2::CENTER_CENTER,
                            label,
                            13.0,
                            color,
                        );
                        if resp.clicked() {
                            *tool = Some(id);
                        }
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
            // Rendu à 320 pt de large (640 px sur un écran 2×) : le recoltage
            // « cover » agrandit la page, 160 pt rendait flou.
            let scale = (320.0 / view.width.max(1.0)).clamp(0.05, 1.0);
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

/// Cadre de la vignette dans sa cellule fixe : UPDF remplit la largeur et
/// recadre le haut de la page (les vignettes de la capture montrent le début
/// du document coupé à droite), pas un « contain » avec des marges.
fn fitted_thumb(cell: Rect, tex_size: [usize; 2]) -> Rect {
    let (w, h) = ((tex_size[0] as f32).max(1.0), (tex_size[1] as f32).max(1.0));
    let scale = (cell.width() / w).max(cell.height() / h);
    Rect::from_min_size(cell.min, vec2(w * scale, h * scale))
}

#[cfg(test)]
mod tests {
    use super::{fitted_thumb, subsequence_score};
    use egui::{pos2, vec2, Rect};

    #[test]
    fn thumb_covers_cell_and_crops_top() {
        let cell = Rect::from_min_size(pos2(0.0, 0.0), vec2(148.0, 118.0));
        // A4 rastérisé à 160 pt de large : 160 × 226. Couverture = largeur
        // pleine, haut aligné, débord vertical coupé par le clip.
        let fitted = fitted_thumb(cell, [160, 226]);
        assert!(fitted.width() < fitted.height(), "la page reste portrait");
        assert!((fitted.width() / fitted.height() - 160.0 / 226.0).abs() < 1e-4);
        assert!((fitted.width() - cell.width()).abs() < 1e-4);
        assert!(fitted.height() > cell.height());
        assert_eq!(fitted.min, cell.min);

        // Paysage : couverture par la hauteur, haut aligné aussi.
        let wide = fitted_thumb(cell, [226, 160]);
        assert!(wide.width() > wide.height());
        assert!(wide.width() >= cell.width());
        assert!((wide.height() - cell.height()).abs() < 1e-4);
        assert_eq!(wide.min, cell.min);
    }

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
        ranked.sort_by_key(|b| std::cmp::Reverse(b.1));
        assert_eq!(ranked.first().map(|(p, _)| *p), Some(&"/tmp/rapport-annuel-2024.pdf"));
    }
}
