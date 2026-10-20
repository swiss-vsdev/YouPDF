//! Fenêtre des préférences et enregistrement automatique.

use super::*;

/// Libellé d'une disposition de pages, pour les menus et les préférences.
pub(super) fn layout_label(layout: PageLayout) -> &'static str {
    match layout {
        PageLayout::Single => "Single page",
        PageLayout::Continuous => "Continuous scroll",
        PageLayout::TwoPages => "Two pages",
        PageLayout::Cover => "Cover (book)",
    }
}

impl YouPdfApp {
    /// Fenêtre « Préférences » : réglages persistants appliqués aux nouveaux
    /// onglets et au démarrage suivant.
    pub(super) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        self.show_modal(ctx, "modal-prefs", "Preferences", 420.0, &mut open, |app, ui| {
            let mut settings = app.ai.settings.clone();
            ui.label(RichText::new(t("General")).size(12.0).color(pal().muted));
            egui::Grid::new("prefs-general")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("On startup").size(11.0));
                    egui::ComboBox::from_id_salt("prefs-startup")
                        .selected_text(match settings.startup.as_str() {
                            "blank" => "Blank document",
                            _ => "Home screen",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut settings.startup,
                                "home".to_string(),
                                "Home screen",
                            );
                            ui.selectable_value(
                                &mut settings.startup,
                                "blank".to_string(),
                                "Blank document",
                            );
                        });
                    ui.end_row();

                    ui.label(RichText::new("Default zoom").size(11.0));
                    egui::ComboBox::from_id_salt("prefs-zoom")
                        .selected_text(settings.default_zoom.clone())
                        .show_ui(ui, |ui| {
                            for option in ["fit-width", "fit-page", "actual"] {
                                ui.selectable_value(
                                    &mut settings.default_zoom,
                                    option.to_string(),
                                    option,
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Default layout").size(11.0));
                    let mut layout = settings.default_layout.min(PageLayout::ALL.len() - 1);
                    egui::ComboBox::from_id_salt("prefs-layout")
                        .selected_text(layout_label(PageLayout::ALL[layout]))
                        .show_ui(ui, |ui| {
                            for (index, candidate) in PageLayout::ALL.iter().enumerate() {
                                if ui
                                    .selectable_label(layout == index, layout_label(*candidate))
                                    .clicked()
                                {
                                    layout = index;
                                }
                            }
                        });
                    settings.default_layout = layout;
                    ui.end_row();

                    ui.label(RichText::new("Author").size(11.0));
                    ui.add(TextEdit::singleline(&mut settings.author).desired_width(180.0));
                    ui.end_row();

                    ui.label(RichText::new("Measure unit").size(11.0));
                    egui::ComboBox::from_id_salt("prefs-unit")
                        .selected_text(settings.measure_unit.clone())
                        .show_ui(ui, |ui| {
                            for option in ["pt", "mm", "cm", "in", "ft", "m", "yd"] {
                                ui.selectable_value(
                                    &mut settings.measure_unit,
                                    option.to_string(),
                                    option,
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Auto-save").size(11.0));
                    egui::ComboBox::from_id_salt("prefs-autosave")
                        .selected_text(match settings.autosave_minutes {
                            0 => "Disabled".to_string(),
                            minutes => format!("{minutes} min"),
                        })
                        .show_ui(ui, |ui| {
                            for minutes in [0u32, 1, 5, 10] {
                                ui.selectable_value(
                                    &mut settings.autosave_minutes,
                                    minutes,
                                    match minutes {
                                        0 => "Disabled".to_string(),
                                        other => format!("{other} min"),
                                    },
                                );
                            }
                        });
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label(RichText::new(t("Appearance")).size(12.0).color(pal().muted));
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Language")).size(11.0));
                let current = Language::from_code(&settings.locale);
                egui::ComboBox::from_id_salt("prefs-language")
                    .selected_text(current.label())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            if ui
                                .selectable_label(current == language, language.label())
                                .clicked()
                            {
                                settings.locale = language.code().to_string();
                                // `selectable_label` ne ferme pas la liste :
                                // sans cela elle reste ouverte après le choix
                                // et masque les boutons Save/Close.
                                ui.close_menu();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                for theme in Theme::ALL {
                    let index = Theme::ALL
                        .iter()
                        .position(|candidate| *candidate == theme)
                        .unwrap_or(0);
                    if ui
                        .selectable_label(settings.theme_index == index, theme.label())
                        .clicked()
                    {
                        settings.theme_index = index;
                        app.theme = theme;
                        app.applied_theme = theme;
                        apply_theme(ctx, theme);
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("AI").size(12.0).color(pal().muted));
            ui.checkbox(&mut settings.ai_enabled, "Enable the AI layer");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    app.ai.settings = settings.clone();
                    let _ = app.ai.settings.save();
                    i18n::set_language(Language::from_code(&app.ai.settings.locale));
                    apply = true;
                }
                if ui.button("Close").clicked() {
                    app.prefs_open = false;
                }
            });
        });
        if apply {
            // Les réglages qui touchent l'onglet courant s'appliquent tout de
            // suite, sans attendre une réouverture.
            self.page_layout = PageLayout::ALL
                .get(self.ai.settings.default_layout)
                .copied()
                .unwrap_or_default();
            if !self.ai.settings.author.trim().is_empty() {
                self.comment_author = Some(self.ai.settings.author.clone());
            }
            self.measure_unit = self.ai.settings.measure_unit.clone();
            self.prefs_open = false;
        }
        if !open {
            self.prefs_open = false;
        }
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
        for tab in &mut self.tabs {
            if !tab.dirty || tab.protected {
                continue;
            }
            let Some(path) = tab.path.clone() else {
                continue;
            };
            let annots = annots_to_page(&tab.doc, &tab.annots);
            if tab.doc.save_with_annotations(&path, &annots).is_ok() {
                tab.dirty = false;
                tab.structural_dirty = false;
                tab.baseline_annots = tab.annots.clone();
                saved += 1;
            }
        }
        if saved > 0 {
            self.toast(ctx, format!("Auto-saved {saved} document(s)"));
        }
    }
}
