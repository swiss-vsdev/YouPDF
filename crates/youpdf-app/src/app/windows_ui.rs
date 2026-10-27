//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel windows_ui).

use super::*;

impl YouPdfApp {

        pub(super) fn toast(&mut self, ctx: &egui::Context, text: impl Into<String>) {
            let now = ctx.input(|i| i.time);
            self.record(text.into(), now);
        }

        /// Affiche une fenêtre modale centrée, avec le chrome commun à tous les
        /// dialogues : non repliable, non redimensionnable, ancrée au centre de
        /// l'écran, fond `chrome_bg` et coins arrondis. Le contenu est dessiné par
        /// `add_contents`, qui reçoit `self` pour pouvoir lire et modifier l'état.
        ///
        /// `max_width == 0.0` laisse la fenêtre s'ajuster à son contenu.
        ///
        /// Les surfaces volontairement redimensionnables (organiser les pages,
        /// comparer, extraire, lot) gardent leur propre `egui::Window` : forcer
        /// leur taille les casserait.
        pub(super) fn show_modal<R>(
            &mut self,
            ctx: &egui::Context,
            id_source: impl std::hash::Hash,
            title: &str,
            max_width: f32,
            open: &mut bool,
            add_contents: impl FnOnce(&mut Self, &mut egui::Ui) -> R,
        ) -> Option<R> {
            let mut result = None;
            egui::Window::new(title)
                .id(Id::new(id_source))
                .open(open)
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .frame(modal_frame(ctx))
                .show(ctx, |ui| {
                    if max_width > 0.0 {
                        ui.set_max_width(max_width);
                    }
                    result = Some(add_contents(self, ui));
                });
            result
        }

        /// Pose un message sans disposer du contexte.
        /// Variante sans `Context` (utilisable avant la première trame) : le
        /// message est horodaté au prochain rendu, comme dans le journal.
        pub(super) fn toast_owned(&mut self, text: impl Into<String>) {
            let text = text.into();
            self.toast = (text.clone(), f64::NAN);
            let duplicate = matches!(self.activity.first(), Some((last, _)) if *last == text);
            if !duplicate {
                self.activity.insert(0, (text, f64::NAN));
                self.activity.truncate(40);
            }
        }

        /// Texture d'une page, nette sur écrans HiDPI.
        ///
        /// `allow_render` est faux pendant un geste de zoom : le palier exact
        /// n'existe pas encore, on étire alors le palier voisin déjà en cache au
        /// lieu de rastériser. Le rendu net est demandé après le geste.
        #[allow(clippy::too_many_arguments)]
        pub(super) fn page_texture(
            ctx: &egui::Context,
            renderer: &dyn Renderer,
            tab: &mut Tab,
            page: u32,
            step: i32,
            logical_scale: f32,
            ppp: f32,
            allow_render: bool,
            fast: bool,
            status_holder: &mut String,
        ) -> Option<TextureHandle> {
            let key = (page, step);
            let target_scale = logical_scale * ppp;
            if let Some(cached) = tab.cache.get(&key) {
                // Au repos, on affine : un palier rendu à une échelle trop
                // éloignée de celle demandée est redessiné. Pendant un geste,
                // l'existant suffit.
                let stale = (cached.scale - target_scale).abs() / target_scale.max(0.001)
                    > RENDER_TOLERANCE;
                if !stale || !allow_render {
                    return Some(cached.texture.clone());
                }
            }

            if !allow_render {
                // Palier voisin : la pyramide a un pas de √2, donc au plus un
                // facteur √2 d'étirement, invisible une fois filtré.
                let nearest = tab
                    .cache
                    .keys()
                    .filter(|(p, s)| *p == page && *s >= 0)
                    .min_by_key(|(_, s)| ((*s - step).abs(), *s))
                    .copied()
                    .and_then(|k| tab.cache.get(&k).map(|c| c.texture.clone()));
                if let Some(tex) = nearest {
                    return Some(tex);
                }
                // Rien en cache pour cette page : elle entre à l'écran. Étirer un
                // néant afficherait une page blanche, on la rastérise donc
                // malgré le geste en cours — mais à basse résolution, sinon la
                // frame saute et le défilement saccade.
            }

            // Pendant un geste, on rasterise à échelle réduite : le bitmap est
            // étiré jusqu'à l'arrêt, puis affiné (l'entrée du cache porte alors
            // une échelle éloignée de la cible, donc elle est reprise).
            let render_scale = if fast {
                target_scale * FAST_RENDER_SCALE
            } else {
                target_scale
            };
            let options = RenderOptions {
                scale: render_scale,
                ..Default::default()
            };
            match renderer.render_page(&tab.doc, page, &options) {
                Ok(bitmap) => {
                    tracing::info!(
                        "render page {page} step {step}: bitmap {}x{} (scale {:.3})",
                        bitmap.width,
                        bitmap.height,
                        render_scale
                    );
                    let image = bitmap.to_egui_image();
                    // La page en cours d'affichage garde ses pixels : la
                    // réécriture y lit la couleur du fond. Une vignette rendue à
                    // une échelle minuscule ne doit pas écraser cette référence.
                    if page as usize == tab.page && step != THUMBNAIL_STEP && !fast {
                        tab.last_bitmap = Some((page, target_scale, bitmap));
                    }
                    let handle = match tab.cache.get(&key) {
                        // Ce palier a déjà une texture : on remplace ses pixels
                        // en gardant le même `TextureId`. `Context::load_texture`,
                        // lui, alloue toujours une texture neuve (son nom n'est
                        // qu'une étiquette de débogage) et laisse l'ancienne au
                        // GPU : chaque zoom en fuyait une.
                        Some(cached) => {
                            let mut texture = cached.texture.clone();
                            texture.set(image, TextureOptions::LINEAR);
                            texture
                        }
                        None => ctx.load_texture(
                            format!("p{page}s{step}"),
                            image,
                            TextureOptions::LINEAR,
                        ),
                    };
                    tab.cache.insert(
                        key,
                        CachedPage {
                            texture: handle.clone(),
                            scale: render_scale,
                        },
                    );

                    // On ne garde que les trois paliers les plus proches de la
                    // page : au-delà ils ne servent plus à étirer, et une longue
                    // séance de zoom en accumulerait des dizaines.
                    let mut steps: Vec<i32> = tab
                        .cache
                        .keys()
                        .filter(|(p, s)| *p == page && *s >= 0)
                        .map(|(_, s)| *s)
                        .collect();
                    if steps.len() > 3 {
                        steps.sort_by_key(|s| (*s - step).abs());
                        for stale in steps.into_iter().skip(3) {
                            tab.cache.remove(&(page, stale));
                        }
                    }

                    Some(handle)
                }
                Err(err) => {
                    *status_holder = format!("Render failed (page {page}): {err}");
                    None
                }
            }
        }

        pub(super) fn rail(&mut self, ctx: &egui::Context) {
            SidePanel::left("rail")
                .exact_width(RAIL_W)
                .resizable(false)
                .frame(Frame::none().fill(pal().chrome_bg))
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        for tool in RAIL {
                            let selected = tool.panel.is_some() && self.panel == tool.panel;
                            let hit = rail_button(ui, selected, tool.draw)
                                .on_hover_text(tool.label);
                            if hit.clicked() {
                                match tool.panel {
                                    Some(p) => {
                                        self.panel = if self.panel == Some(p) { None } else { Some(p) }
                                    }
                                    None => self.organize_open = true,
                                }
                            }
                            ui.add_space(10.0);
                        }

                        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                            ui.add_space(10.0);
                            let selected = self.panel == Some(Panel::Aspect);
                            if rail_button(ui, selected, icon_aspect).clicked() {
                                self.panel =
                                    if selected { None } else { Some(Panel::Aspect) };
                            }
                        });
                    });
                });
        }

        /// Fenêtre « Propriétés de la page ».
        pub(super) fn page_props_window(&mut self, ctx: &egui::Context) {
            let Some(number) = self.page_props else {
                return;
            };
            let Some(tab) = self.tabs.get(self.active) else {
                self.page_props = None;
                return;
            };
            let total = tab.total();
            if number == 0 || number > total {
                self.page_props = None;
                return;
            }
            let view = tab.doc.page_view(number as u32);
            let (dw, dh) = tab.doc.page_size(number as u32);
            let boxes = tab.doc.page_boxes(number as u32);
            let mut open = true;
            self.show_modal(
                ctx,
                "modal-page-props",
                "Page properties",
                320.0,
                &mut open,
                |_app, ui| {
                    let mm = |pt: f32| pt * 25.4 / 72.0;
                    let mut row = |label: &str, value: String| {
                        ui.horizontal(|ui| {
                            ui.label(section(label));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(value));
                            });
                        });
                    };
                    row("Page", format!("{number} of {total}"));
                    row(
                        "Visible box",
                        format!("{:.0} × {:.0} pt", view.width, view.height),
                    );
                    row(
                        "Paper size",
                        format!("{:.1} × {:.1} mm", mm(view.width), mm(view.height)),
                    );
                    row("Displayed", format!("{dw:.0} × {dh:.0} pt"));
                    row("Rotation", format!("{}°", view.rotation));
                    row("Origin", format!("({:.0}, {:.0})", view.x, view.y));

                    // Boîtes déclarées (Media / Crop / Trim / Bleed), quand elles
                    // existent : Media est le support, Crop la zone visible.
                    let mut any = false;
                    for (label, rect) in [
                        ("Media box", boxes.media),
                        ("Crop box", boxes.crop),
                        ("Trim box", boxes.trim),
                        ("Bleed box", boxes.bleed),
                    ] {
                        if let Some([x1, y1, x2, y2]) = rect {
                            row(
                                label,
                                format!(
                                    "{:.0} {:.0} {:.0} {:.0} pt",
                                    x1,
                                    y1,
                                    (x2 - x1).abs(),
                                    (y2 - y1).abs()
                                ),
                            );
                            any = true;
                        }
                    }
                    if !any {
                        ui.label(
                            RichText::new(t("No page boxes declared"))
                                .size(LABEL)
                                .color(pal().muted),
                        );
                    }
                },
            );
            if !open {
                self.page_props = None;
            }
        }

        /// Fenêtre « Propriétés du document » (lecture seule).
        pub(super) fn properties_window(&mut self, ctx: &egui::Context) {
            if !self.props_open {
                return;
            }
            let Some(tab) = self.tabs.get(self.active) else {
                self.props_open = false;
                return;
            };
            let meta = tab.doc.metadata().clone();
            let total = tab.total();
            let path = tab
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(not saved yet)".to_owned());
            let size = tab
                .path
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| format!("{:.1} KB", m.len() as f64 / 1024.0))
                .unwrap_or_else(|| "—".to_owned());
            let stats = youpdf_core::search::DocumentStats::from_index(
                &youpdf_core::SearchIndex::build(&tab.doc).expect("build index"),
            );
            let perms = youpdf_core::read_permissions(&tab.doc);
            let mut open = true;
            let title = tab.title.clone();

            self.show_modal(ctx, "modal-properties", "Document properties", 0.0, &mut open, |_app, ui| {
                let mut row = |label: &str, value: &str| {
                    ui.horizontal(|ui| {
                        ui.label(section(label));
                        ui.label(RichText::new(value).color(pal().icon_bright));
                    });
                };
                row("File", &title);
                row("Path", &path);
                row("Size", &size);
                row("Pages", &total.to_string());
                row("Title", meta.title.as_deref().unwrap_or("—"));
                row("Author", meta.author.as_deref().unwrap_or("—"));
                row("Subject", meta.subject.as_deref().unwrap_or("—"));
                row("Keywords", meta.keywords.as_deref().unwrap_or("—"));
                row("Producer", meta.producer.as_deref().unwrap_or("—"));
                row("Word count", &stats.words.to_string());
                row("CJK", &stats.cjk.to_string());
                row("Spaces", &stats.spaces.to_string());
                row("Total chars", &stats.total_chars.to_string());
                if let Some(perms) = &perms {
                    ui.add_space(SPACE_S);
                    ui.label(section("Permissions"));
                    for perm in perms {
                        let value = if perm.allowed { "Allowed" } else { "Denied" };
                        ui.horizontal(|ui| {
                            ui.label(section(perm.label));
                            ui.label(RichText::new(value).color(pal().icon_bright));
                        });
                    }
                }
            });

            if !open {
                self.props_open = false;
            }
        }

        /// Peint le panneau IA. Le texte du document n'est extrait qu'une fois par
        /// onglet, et seulement quand le panneau est ouvert.
        pub(super) fn ai_window(&mut self, ctx: &egui::Context) {
            if !self.ai.open {
                return;
            }
            if self.ai.enabled() && self.ai_doc_tab != Some(self.active) {
                self.ai_doc_tab = Some(self.active);
                self.ai_doc_text = self
                    .tabs
                    .get(self.active)
                    .map(|tab| extract_document_text(&tab.doc))
                    .unwrap_or_default();
            }
            let (name, pages) = match self.tabs.get(self.active) {
                Some(tab) => (Some(tab.title.clone()), Some(tab.total())),
                None => (None, None),
            };
            self.ai.show(
                ctx,
                self.tabs.get(self.active).map(|tab| &tab.doc),
                &self.ai_doc_text,
                name.as_deref(),
                pages,
            );
            // La fenêtre de traduction a besoin du document actif, et de
            // l'application qui possède le document mutable : c'est elle qui
            // écrit le résultat dans l'onglet.
            let doc_ref = self.tabs.get(self.active).map(|tab| &tab.doc);
            self.ai.translate_window(ctx, doc_ref);
            if self.ai.translate_apply.take() == Some(true) {
                self.apply_translation(ctx);
            }
            if let Some(page) = self.ai.goto_page.take() {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.page = page;
                }
                self.jump = Some(page);
            }
        }

        /// Applique la traduction bilingale stockée dans le panneau IA au
    /// document actif.
    ///
    /// C'est l'interface qui écrit le résultat : le fil de travail a seulement
    /// traduit le texte, page par page. Le document est modifié en place par
    /// [`Document::translate_page`], qui utilise la primitive de caviardage
    /// pour effacer l'original et poser la traduction à un corps réduit.
    ///
    /// ponytail: page-level blocks only. Les boîtes par fragment de texte
    /// nécessiteraient un extracteur de flux de contenu ; sans elles, la
    /// traduction recouvre toute la page et réduit le corps jusqu'à ce que
    /// la traduction tienne. Les documents multi-colonnes ou à tableaux
    /// ne survivent pas à l'opération telle qu'elle est implémentée.
    pub(super) fn apply_translation(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let renderer = &*self.renderer;
        let result = self.ai.apply_translation(&mut tab.doc, renderer);
        match result {
            Ok(()) => {
                self.mark_document_changed();
                self.toast(ctx, "Translation applied to the document.");
            }
            Err(err) => {
                self.toast(ctx, format!("Translation failed: {err}"));
            }
        }
    }

    /// Fenêtre « Créer » : page vierge ou PDF à partir d'images.
        pub(super) fn create_window(&mut self, ctx: &egui::Context) {
            if !self.create_open {
                return;
            }
            let mut open = true;
            let mut make_blank = false;
            let mut images = false;
            let mut capture: Option<bool> = None;
            self.show_modal(ctx, "modal-create", "Create", 380.0, &mut open, |app, ui| {
                ui.label(section(t("Blank document")));
                ui.horizontal(|ui| {
                    ui.label(section(t("Pages")));
                    ui.add(egui::DragValue::new(&mut app.create_pages).range(1..=500));
                    if ui.button(t("Create blank")).clicked() {
                        make_blank = true;
                    }
                });
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(section(t("From images")));
                ui.label(
                    RichText::new(
                        "One page per image, in the order chosen. PNG and JPEG are read.",
                    )
                    .size(LABEL)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_S);
                if ui.button(t("Choose images…")).clicked() {
                    images = true;
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(section(t("From capture")));
                ui.label(
                    RichText::new("macOS only: the screen selection crosshair, or a PNG image already on the clipboard.")
                        .size(LABEL)
                        .color(pal().muted),
                );
                ui.add_space(SPACE_S);
                ui.horizontal(|ui| {
                    if ui.button(t("Screen selection…")).clicked() {
                        capture = Some(false);
                    }
                    if ui.button(t("Clipboard")).clicked() {
                        capture = Some(true);
                    }
                });
            });

            if let Some(from_clipboard) = capture {
                self.import_capture(ctx, from_clipboard);
            }
            if make_blank {
                match blank_document(self.create_pages.max(1), 595.0, 842.0) {
                    Ok(doc) => {
                        self.push_document(doc, None);
                        self.create_open = false;
                    }
                    Err(err) => self.toast(ctx, format!("Create failed: {err}")),
                }
            }
            if images {
                self.import_images(ctx);
            }
            if !open {
                self.create_open = false;
            }
        }

        /// Fond du canevas : la teinte du thème pour le réglage par défaut, le
        /// choix explicite du panneau Aspect sinon.
        pub(super) fn canvas_background(&self) -> Color32 {
            match self.canvas_style {
                // 0 = Auto : le canevas suit le thème de l'application.
                0 => pal().canvas_bg,
                index => CANVAS_THEMES[(index - 1).min(CANVAS_THEMES.len() - 1)].1,
            }
        }

        /// Surface d'accueil : accès direct aux fichiers récents.
        pub(super) fn empty_state(&mut self, ui: &mut egui::Ui) {
            self.home_screen(ui);
        }

        /// Journal d'activité (cloche de la barre de titre) : les derniers
        /// messages affichés, du plus récent au plus ancien.
        pub(super) fn activity_window(&mut self, ctx: &egui::Context) {
            if !self.activity_open {
                return;
            }
            let now = ctx.input(|i| i.time);
            let mut open = true;
            let mut clear = false;

            egui::Window::new(t("Activity"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(320.0)
                .anchor(Align2::RIGHT_TOP, vec2(-14.0, TITLE_H + 10.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(t("What YouPDF just did"))
                                .size(LABEL)
                                .color(pal().muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Clear").clicked() {
                                clear = true;
                            }
                        });
                    });
                    ui.add_space(SPACE_S);
                    if self.activity.is_empty() {
                        ui.label(
                            RichText::new(t("Nothing yet."))
                                .size(BODY)
                                .color(pal().muted),
                        );
                        return;
                    }
                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (text, stamp) in &self.activity {
                            let age = (now - stamp).max(0.0);
                            let when = if stamp.is_nan() || age < 60.0 {
                                "just now".to_owned()
                            } else if age < 3600.0 {
                                format!("{:.0} min ago", age / 60.0)
                            } else {
                                format!("{:.0} h ago", age / 3600.0)
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(text).size(BODY));
                                ui.label(caption(when));
                            });
                            ui.add_space(SPACE_XS);
                        }
                    });
                });

            if clear {
                self.activity.clear();
            }
            if !open {
                self.activity_open = false;
            }
        }

        /// Message éphémère en bas au centre (5 s).
        pub(super) fn toast_overlay(&mut self, ctx: &egui::Context) {
            let (text, mut stamp) = self.toast.clone();
            if text.is_empty() {
                return;
            }
            let now = ctx.input(|i| i.time);
            if stamp.is_nan() {
                stamp = now;
                self.toast.1 = now;
            }
            if now - stamp > 5.0 {
                return;
            }

            Area::new(Id::new("toast"))
                .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(0x28, 0x28, 0x28, 0xF2))
                        .rounding(Rounding::same(RADIUS_L))
                        .inner_margin(Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&text).size(BODY).color(pal().icon_bright))
                                    .extend(),
                            );
                        });
                });
        }

}
