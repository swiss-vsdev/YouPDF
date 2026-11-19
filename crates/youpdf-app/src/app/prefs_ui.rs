//! Fenêtre des préférences : barre latérale de catégories et cartes de
//! réglages (calquée sur la fenêtre UPDF 2.5.6).

use super::*;
use youpdf_shared::{LABEL, TITLE, darken, t, RADIUS_L, RADIUS_TINY, RADIUS_XS, RADIUS_XL, SPACE_M};

const SIDEBAR_W: f32 = 116.0;
const TABS: [(&str, IconFn); 6] = [
    ("General", icon_gear),
    ("Appearance", icon_aspect),
    ("Documents", icon_page),
    ("Comments", icon_bubble),
    ("Measurements", icon_measure),
    ("AI", icon_sparkle),
];

/// Interrupteur UPDF : pill coloré et pastille blanche.
fn switch(ui: &mut egui::Ui, on: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 19.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        Rounding::same(9.5), // ponytail: rayon = moitié de la hauteur (pill), pas un jeton d'échelle
        if on { pal().accent } else { pal().hover_bg },
    );
    let x = if on { rect.max.x - 9.5 } else { rect.min.x + 9.5 };
    // Pastille blanche sur la piste colorée, sombre sur la piste éteinte :
    // un blanc fixe disparaissait sur la piste claire des thèmes Light/Mint.
    let knob = if on { Color32::WHITE } else { pal().icon_bright };
    ui.painter()
        .circle_filled(pos2(x, rect.center().y), 7.5, knob);
    resp.clicked()
}

/// Une section : titre, filet, puis carte au fond légèrement plus clair.
fn card(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title).size(TITLE).strong());
    ui.add_space(4.0);
    let line = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover()).0;
    ui.painter()
        .hline(line.x_range(), line.min.y, Stroke::new(1.0_f32, pal().separator));
    ui.add_space(8.0);
    Frame::none()
        .fill(pal().widget_bg)
        .stroke(Stroke::new(1.0_f32, pal().separator))
        .rounding(Rounding::same(RADIUS_XL))
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, content);
    ui.add_space(14.0);
}

/// Ligne « libellé + statut + interrupteur » des préférences UPDF.
fn toggle_row(ui: &mut egui::Ui, label: &str, on: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        // Le libellé prend l'espace restant et se tronque : le statut et
        // l'interrupteur sont dessinés à droite sans chevauchement.
        ui.add(egui::Label::new(RichText::new(label).size(TITLE)).truncate());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = switch(ui, on);
            ui.add_space(8.0);
            ui.label(
                RichText::new(if on { t("On") } else { t("Off") })
                    .size(LABEL)
                    .color(pal().muted),
            );
        });
    });
    changed
}

/// Vignette de thème : mini-page avec en-tête coloré, libellé, anneau d'actif.
fn theme_tile(ui: &mut egui::Ui, label: &str, color: Color32, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(92.0, 88.0), Sense::click());
    let body = rect.shrink(2.0);
    ui.painter()
        .rect_filled(body, Rounding::same(RADIUS_L), pal().hover_bg);
    let mock = Rect::from_min_size(body.min + vec2(8.0, 8.0), vec2(body.width() - 16.0, 44.0));
    ui.painter()
        .rect_filled(mock, Rounding::same(RADIUS_TINY), Color32::from_rgb(0xE8, 0xE8, 0xEA));
    ui.painter().rect_filled(
        Rect::from_min_size(mock.min, vec2(mock.width(), 12.0)),
        Rounding::same(RADIUS_TINY),
        color,
    );
    for i in 0..3 {
        ui.painter().rect_filled(
            Rect::from_min_size(
                mock.min + vec2(7.0, 17.0 + i as f32 * 9.0),
                vec2(mock.width() - 18.0 - i as f32 * 6.0, 4.0),
            ),
            Rounding::same(RADIUS_XS),
            Color32::from_rgb(0xB8, 0xBC, 0xC4),
        );
    }
    ui.painter().text(
        pos2(rect.center().x, rect.max.y - 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        if selected { pal().accent } else { pal().muted },
    );
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            Rounding::same(RADIUS_XL),
            Stroke::new(2.0_f32, pal().accent),
        );
    }
    resp.clicked()
}

/// Deux vignettes « une page / deux pages » de l'onglet Affichage.
fn layout_tile(ui: &mut egui::Ui, two: bool, selected: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(126.0, 78.0), Sense::click());
    let body = rect.shrink(2.0);
    ui.painter()
        .rect_filled(body, Rounding::same(RADIUS_L), pal().hover_bg);
    let page = |x: f32, w: f32| {
        let r = Rect::from_min_size(
            pos2(body.min.x + x, body.min.y + 8.0),
            vec2(w, body.height() - 16.0),
        );
        ui.painter()
            .rect_filled(r, Rounding::same(RADIUS_XS), Color32::WHITE);
        for i in 0..3 {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    r.min + vec2(5.0, 8.0 + i as f32 * 10.0),
                    vec2(r.width() - 10.0, 4.0),
                ),
                Rounding::same(RADIUS_XS),
                Color32::from_rgb(0xC4, 0xC7, 0xCE),
            );
        }
    };
    if two {
        page(body.width() * 0.18, body.width() * 0.27);
        page(body.width() * 0.53, body.width() * 0.27);
    } else {
        page(body.width() * 0.32, body.width() * 0.36);
    }
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            Rounding::same(RADIUS_XL),
            Stroke::new(2.0_f32, pal().accent),
        );
    }
    resp.clicked()
}

/// Barre latérale : feux de circulation factices puis catégories.
fn sidebar(ui: &mut egui::Ui, tab: &mut usize) {
    // La couleur de fond est peinte sur le rect existant : ne pas l'allouer,
    // sinon le curseur saute en bas et les onglets se retrouvent dessous.
    ui.set_min_width(SIDEBAR_W);
    let rect = ui.max_rect();
    ui.painter()
        .rect_filled(rect, Rounding::ZERO, darken(pal().chrome_bg, 0x04));
    ui.add_space(14.0);
    for (index, (label, icon)) in TABS.iter().enumerate() {
        let active = *tab == index;
        let (item, resp) =
            ui.allocate_exact_size(vec2(SIDEBAR_W - 14.0, 32.0), Sense::click());
        if active {
            ui.painter()
                .rect_filled(item, Rounding::same(RADIUS_L), pal().accent);
        }
        let color = if active { Color32::WHITE } else { pal().icon };
        icon(
            ui.painter(),
            Rect::from_center_size(pos2(item.min.x + 17.0, item.center().y), vec2(16.0, 16.0)),
            color,
        );
        ui.painter().text(
            pos2(item.min.x + 34.0, item.center().y),
            Align2::LEFT_CENTER,
            t(label),
            FontId::proportional(12.0),
            color,
        );
        if resp.clicked() {
            *tab = index;
        }
        ui.add_space(5.0);
    }
}

/// Contenu de la catégorie sélectionnée.
fn category(ui: &mut egui::Ui, app: &mut YouPdfApp, settings: &mut youpdf_ai::AppSettings, tab: usize) {
    match tab {
        // ---- Générales : aspect + corps (capture 08.24.05) --------------
        0 => {
            card(ui, t("Appearance"), |ui| {
                ui.horizontal(|ui| {
                    // Ordre strictement identique à Theme::ALL :
                    // [Dark, Light, Mint, Starry].
                    // La vignette montre le vrai accent du thème, pas une
                    // couleur décorative inventée qui ne correspond à rien.
                    for (index, theme) in Theme::ALL.iter().copied().enumerate() {
                        if theme_tile(ui, theme.label(), theme.palette().accent,
                            settings.theme_index == index)
                        {
                            settings.theme_index = index;
                            let theme = Theme::ALL[index];
                            app.theme = theme;
                            // Ne pas poser `applied_theme` ici : ce `ctx` est
                            // celui du popup (fenêtre native). `sync_theme` du
                            // prochain frame appliquera le thème au ctx principal.
                            apply_theme(ui.ctx(), theme);
                        }
                    }
                });
            });
            card(ui, t("General"), |ui| {
                ui.label(RichText::new(t("On startup")).size(LABEL));
                egui::ComboBox::from_id_salt("prefs-startup")
                    .selected_text(if settings.startup == "blank" { "Blank document" } else { "Home screen" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut settings.startup, "home".to_string(), "Home screen");
                        ui.selectable_value(&mut settings.startup, "blank".to_string(), "Blank document");
                    });
                ui.add_space(SPACE_M);
                ui.label(RichText::new(t("Language")).size(LABEL));
                let current = Language::from_code(&settings.locale);
                egui::ComboBox::from_id_salt("prefs-language")
                    .selected_text(current.label())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            if ui.selectable_label(current == language, language.label()).clicked() {
                                settings.locale = language.code().to_string();
                                ui.close_menu();
                            }
                        }
                    });
            });
        }
        // ---- Affichage : mise en page et bascules (capture 08.24.37) ----
        1 => {
            card(ui, t("Default page layout"), |ui| {
                ui.label(RichText::new(t("Page display")).size(LABEL));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let layout = settings.default_layout.min(PageLayout::ALL.len() - 1);
                    if layout_tile(ui, false, layout != PageLayout::TwoPages as usize
                        && layout != PageLayout::Cover as usize)
                    {
                        settings.default_layout = PageLayout::Single as usize;
                    }
                    ui.add_space(10.0);
                    if layout_tile(ui, true, layout == PageLayout::TwoPages as usize
                        || layout == PageLayout::Cover as usize)
                    {
                        settings.default_layout = PageLayout::TwoPages as usize;
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.radio_value(&mut settings.default_layout, PageLayout::Single as usize, t("Single page"));
                    ui.radio_value(&mut settings.default_layout, PageLayout::TwoPages as usize, t("Two pages"));
                });
                ui.add_space(8.0);
                if toggle_row(ui, t("Enable scrolling"),
                    settings.default_layout == PageLayout::Continuous as usize)
                {
                    settings.default_layout = if settings.default_layout == PageLayout::Continuous as usize {
                        PageLayout::Single as usize
                    } else {
                        PageLayout::Continuous as usize
                    };
                }
                let mut cover = app.page_display.show_cover_page;
                if toggle_row(ui, t("Show cover page in two-page view"), cover) {
                    cover = !cover;
                    app.page_display.show_cover_page = cover;
                }
                if toggle_row(ui, t("Enable Liquid Glass"), settings.liquid_glass) {
                    settings.liquid_glass = !settings.liquid_glass;
                }
            });
        }
        // ---- Documents --------------------------------------------------
        2 => {
            card(ui, t("Documents"), |ui| {
                ui.label(RichText::new(t("Default zoom")).size(LABEL));
                egui::ComboBox::from_id_salt("prefs-zoom")
                    .selected_text(settings.default_zoom.clone())
                    .show_ui(ui, |ui| {
                        for (value, label) in [("fit-width", "Fit width"), ("fit-page", "Fit page"), ("actual", "Actual size")] {
                            ui.selectable_value(&mut settings.default_zoom, value.to_string(), label);
                        }
                    });
                ui.add_space(SPACE_M);
                ui.label(RichText::new(t("Auto-save")).size(LABEL));
                egui::ComboBox::from_id_salt("prefs-autosave")
                    .selected_text(match settings.autosave_minutes { 0 => "Disabled".to_string(), m => format!("{m} min") })
                    .show_ui(ui, |ui| {
                        for minutes in [0u32, 1, 5, 10] {
                            ui.selectable_value(&mut settings.autosave_minutes, minutes,
                                if minutes == 0 { "Disabled".to_string() } else { format!("{minutes} min") });
                        }
                    });
            });
        }
        // ---- Commentaires ----------------------------------------------
        3 => {
            card(ui, t("Comments"), |ui| {
                ui.label(RichText::new(t("Author")).size(LABEL));
                ui.add(TextEdit::singleline(&mut settings.author).desired_width(220.0));
            });
        }
        // ---- Mesures ----------------------------------------------------
        4 => {
            card(ui, t("Measurements"), |ui| {
                ui.label(RichText::new(t("Measure unit")).size(LABEL));
                egui::ComboBox::from_id_salt("prefs-unit")
                    .selected_text(settings.measure_unit.clone())
                    .show_ui(ui, |ui| {
                        for option in ["pt", "mm", "cm", "in", "ft", "m", "yd"] {
                            ui.selectable_value(&mut settings.measure_unit, option.to_string(), option);
                        }
                    });
            });
        }
        // ---- IA ---------------------------------------------------------
        _ => {
            card(ui, t("AI"), |ui| {
                if toggle_row(ui, t("Enable the AI layer"), settings.ai_enabled) {
                    settings.ai_enabled = !settings.ai_enabled;
                }
                ui.add_space(SPACE_M);
                ui.label(
                    RichText::new(t("Ollama runs locally and needs no API key."))
                        .size(LABEL)
                        .color(pal().muted),
                );
            });
        }
    }
}

impl YouPdfApp {
    /// Fenêtre « Préférences » : barre latérale UPDF + cartes de réglages.
    pub(super) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_open {
            return;
        }
        let mut open = true;
        let original_theme = self.theme;
        let draft = self
            .prefs_draft
            .take()
            .unwrap_or_else(|| self.ai.settings.clone());
        let result = Popup::window(t("Preferences…"))
            .id(Id::new("modal-prefs"))
            .fixed()
            .default_size(vec2(540.0, 380.0))
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, &mut open, |ui| {
                let mut settings = draft;
                let mut tab = self.prefs_tab;
                ui.set_min_height(380.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| sidebar(ui, &mut tab));
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        let mut action = (settings.clone(), false, false);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                category(ui, self, &mut settings, tab);
                            });
                        // Boutons fixes en bas, hors du défilement.
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(t("Save")).clicked() {
                                action = (settings.clone(), true, false);
                            }
                            if ui.button(t("Close")).clicked() {
                                action = (settings.clone(), false, true);
                            }
                        });
                        self.prefs_tab = tab;
                        action
                    })
                    .inner
                })
                .inner
            });

        let (settings, save, close) = match result {
            Some(value) => value,
            None => (self.ai.settings.clone(), false, false),
        };

        if close || !open {
            // Le clic sur une vignette applique le thème tout de suite ; un
            // « Close » sans « Save » ne doit pas le laisser appliqué.
            if self.theme != original_theme {
                self.theme = original_theme;
                self.sync_theme(ctx);
            }
            self.prefs_open = false;
            self.prefs_draft = None;
            return;
        }
        if save {
            self.ai.settings = settings.clone();
            let _ = self.ai.settings.save();
            i18n::set_language(Language::from_code(&self.ai.settings.locale));
            self.prefs_open = false;
            self.prefs_draft = None;
            self.page_layout = PageLayout::ALL
                .get(self.ai.settings.default_layout)
                .copied()
                .unwrap_or_default();
            if !self.ai.settings.author.trim().is_empty() {
                self.comment_author = Some(self.ai.settings.author.clone());
            }
            self.measure_unit = self.ai.settings.measure_unit.clone();
            return;
        }
        self.prefs_draft = Some(settings);
    }

    /// Enregistre les onglets modifiés à intervalle régulier.
    pub(super) fn autosave_tick(&mut self, ctx: &egui::Context) {
        let minutes = self.ai.settings.autosave_minutes;
        if minutes == 0 {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - self.autosave_at < minutes as f64 * 60.0 {
            return;
        }
        self.autosave_at = now;
        let mut saved = 0usize;
        let mut failed = 0usize;
        for tab in &mut self.tabs {
            if !tab.dirty || tab.protected {
                continue;
            }
            let Some(path) = tab.path.clone() else {
                continue;
            };
            let annots = youpdf_core::annotation::transform::annots_to_page(&tab.doc, &tab.annots);
            match tab.doc.save_with_annotations(&path, &annots) {
                Ok(()) => {
                    tab.dirty = false;
                    tab.structural_dirty = false;
                    tab.baseline_annots = annots;
                    tab.history_base = tab.doc.to_bytes().ok();
                    tab.saved_hash = tab
                        .history_base
                        .as_deref()
                        .map(fingerprint)
                        .unwrap_or_default();
                    saved += 1;
                }
                Err(err) => {
                    failed += 1;
                    tracing::warn!(error = %err, path = %path.display(), "autosave failed");
                }
            }
        }
        if saved > 0 {
            self.toast(ctx, format!("Auto-saved {saved} document(s)"));
        }
        if failed > 0 {
            self.toast(
                ctx,
                format!("Auto-save failed for {failed} document(s) — save manually to retry"),
            );
        }
    }
}
