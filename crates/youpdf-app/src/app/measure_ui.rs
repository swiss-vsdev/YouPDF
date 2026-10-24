//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel measure_ui).


use super::*;

impl YouPdfApp {

        /// Fenêtre « Mesure » : nature, échelle, unité, activation.
        pub(super) fn measure_window(&mut self, ctx: &egui::Context) {
            if !self.measure_open {
                return;
            }
            let mut open = true;
            let mut clear = false;
            self.show_modal(ctx, "modal-measure", "Measure", 340.0, &mut open, |app, ui| {
                ui.checkbox(&mut app.measure_active, t("Draw on the page"));
                ui.add_space(SPACE_M);
                ui.label(section(t("Kind")));
                for kind in [
                    MeasureKind::Distance,
                    MeasureKind::Perimeter,
                    MeasureKind::Area,
                ] {
                    if ui
                        .selectable_label(app.measure_kind == kind, kind.display_name())
                        .clicked()
                    {
                        app.measure_kind = kind;
                    }
                }
                ui.add_space(SPACE_M);
                egui::Grid::new("measure-grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(section(t("Scale")));
                        ui.add(
                            egui::DragValue::new(&mut app.measure_scale)
                                .range(0.01..=10_000.0)
                                .speed(1.0),
                        );
                        ui.end_row();
                        ui.label(section(t("Unit")));
                        ui.add(
                            TextEdit::singleline(&mut app.measure_unit).desired_width(80.0),
                        );
                        ui.end_row();
                        ui.label(section(t("Known")));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut app.measure_known)
                                    .range(0.0001..=1_000_000.0)
                                    .speed(0.1),
                            );
                            if ui
                                .add_enabled(
                                    app.measure_last_len.is_some(),
                                    egui::Button::new("Calibrate"),
                                )
                                .on_hover_text(
                                    "Use the last drawn distance as the known real distance",
                                )
                                .clicked()
                            {
                                if let Some(length) = app.measure_last_len {
                                    app.measure_scale = length / app.measure_known.max(0.0001);
                                }
                            }
                        });
                        ui.end_row();
                    });
                ui.add_space(SPACE_M);
                ui.label(
                    RichText::new(
                        "Scale is drawing points per real unit: 72 with unit \"in\" reads inches \
                         at 1:1.",
                    )
                    .size(LABEL)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_S);
                ui.label(
                    RichText::new(
                        "Distance follows the drag; perimeter and area take the dragged \
                         rectangle.",
                    )
                    .size(LABEL)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_L);
                if ui.button(t("Remove all measurements")).clicked() {
                    clear = true;
                }
            });
            if clear {
                let result = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.clear_measurements());
                match result {
                    Some(Ok(count)) => {
                        self.mark_document_changed();
                        self.toast(ctx, format!("Removed {count} measurement(s)"));
                    }
                    Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                    None => {}
                }
            }
            if !open {
                self.measure_open = false;
                // Sans la fenêtre, l'outil n'a plus de réglages visibles : le
                // laisser actif bloquerait tous les autres outils du canevas.
                self.measure_active = false;
            }
        }

}
