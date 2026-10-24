//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel aspect_ui).


use super::*;

impl YouPdfApp {

        /// Panneau Aspect : fond du canevas, ombre, taille des vignettes.
        pub(super) fn aspect_panel(&mut self, ui: &mut egui::Ui) {
            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(section(t("Background")));
            });
            ui.add_space(SPACE_M);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                // « Auto » puis les quatre fonds nommés.
                let swatches: Vec<(&str, Color32)> = std::iter::once(("Auto", pal().canvas_bg))
                    .chain(CANVAS_THEMES.iter().copied())
                    .collect();
                for (idx, (name, color)) in swatches.into_iter().enumerate() {
                    let selected = self.canvas_style == idx;
                    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), color);
                    if selected {
                        ui.painter().rect_stroke(
                            rect.expand(2.0),
                            7.0,
                            Stroke::new(1.5_f32, pal().accent),
                        );
                    }
                    if resp.clicked() {
                        self.canvas_style = idx;
                    }
                    resp.on_hover_text(name);
                }
            });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(section(t("Page tint")));
            });
            ui.add_space(SPACE_M);
            // Aperçu : chaque pastille montre la teinte posée sur du blanc,
            // comme sur la page.
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                for (idx, (name, rgba)) in PAGE_TINTS.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                    ui.painter()
                        .rect_filled(rect, Rounding::same(RADIUS_S), Color32::WHITE);
                    ui.painter().rect_filled(
                        rect,
                        Rounding::same(RADIUS_S),
                        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
                    );
                    if self.page_tint == idx {
                        ui.painter().rect_stroke(
                            rect.expand(2.0),
                            5.0,
                            Stroke::new(1.5_f32, pal().accent),
                        );
                    }
                    if resp.clicked() {
                        self.page_tint = idx;
                    }
                    resp.on_hover_text(*name);
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let mut border = self.page_border;
                if ui.checkbox(&mut border, t("Page border")).changed() {
                    self.page_border = border;
                }
            });

            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let mut shadow = self.page_shadow;
                if ui.checkbox(&mut shadow, t("Page shadow")).changed() {
                    self.page_shadow = shadow;
                }
            });

            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(section(t("Thumbnails")));
            });
            ui.add_space(SPACE_S);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add_sized(
                    vec2(self.panel_w - 40.0, 18.0),
                    egui::Slider::new(&mut self.thumb_scale, 0.6..=1.6)
                        .show_value(false),
                );
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(t("Notation"))
                        .size(LABEL)
                        .color(pal().muted),
                );
            });
            ui.add_space(SPACE_S);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(t("Colours come from the current tool presets."))
                        .size(CAPTION)
                        .color(pal().muted),
                );
            });
        }

}
