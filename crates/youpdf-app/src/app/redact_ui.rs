//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel redact_ui).


use super::*;
use youpdf_core::{DEFAULT_CODE_SETS, mark_code_matches};

impl YouPdfApp {

        /// Fenêtre de caviardage, en deux temps.
        ///
        /// Première phase : le marquage. On choisit l'apparence des zones — le
        /// remplissage, la couleur, le texte superposé. Deuxième phase : la
        /// confirmation, qui dit honnêtement ce que le moteur va faire.
        pub(super) fn redact_window(&mut self, ctx: &egui::Context) {
            if !self.redact_modal {
                return;
            }
            let areas = self.redact_marks.len();
            let mut confirm = false;
            let mut cancel = false;
            let mut advance = false;
            let mut keep_open = true;
            let phase = self.redact_phase;
            self.show_modal(ctx, "modal-redact", "Redaction", 420.0, &mut keep_open, |app, ui| {
                if phase == RedactPhase::Marking {
                    ui.label(
                        RichText::new(format!("{areas} area(s) marked."))
                            .size(BODY)
                            .color(pal().icon_bright),
                    );
                    ui.add_space(SPACE_L);

                    ui.label(muted(t("Fill"), LABEL));
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut app.redact_fill,
                            RedactionFill::Black,
                            "Black",
                        );
                        ui.selectable_value(
                            &mut app.redact_fill,
                            RedactionFill::White,
                            "White",
                        );
                        ui.selectable_value(
                            &mut app.redact_fill,
                            RedactionFill::Custom(app.redact_custom_color),
                            "Custom",
                        );
                    });
                    if matches!(app.redact_fill, RedactionFill::Custom(_)) {
                        let mut color = Color32::from_rgb(
                            app.redact_custom_color[0],
                            app.redact_custom_color[1],
                            app.redact_custom_color[2],
                        );
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut color);
                            if ui.button(t("Use colour")).clicked() {
                                app.redact_custom_color = [color.r(), color.g(), color.b()];
                                app.redact_fill = RedactionFill::Custom(app.redact_custom_color);
                            }
                        });
                    }

                    ui.add_space(SPACE_L);
                    ui.label(muted(t("Overlay text"), LABEL));
                    ui.add(
                        egui::TextEdit::singleline(&mut app.redact_overlay_text)
                            .hint_text("[REDACTED]")
                            .desired_width(f32::INFINITY),
                    );
                    if !app.redact_overlay_text.is_empty() {
                        ui.horizontal(|ui| {
                            for font in PatchFont::ALL {
                                ui.selectable_value(
                                    &mut app.redact_overlay_font,
                                    font,
                                    font.display_name(),
                                );
                            }
                            ui.add(
                                egui::DragValue::new(&mut app.redact_overlay_size)
                                    .range(6.0..=72.0)
                                    .suffix(" pt"),
                            );
                        });
                    }

                    ui.add_space(12.0);
                    ui.collapsing("Code Sets", |ui| {
                        ui.label(
                            RichText::new("Detect sensitive data patterns and auto-mark them.")
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        ui.add_space(SPACE_XS);
                        for (idx, entry) in DEFAULT_CODE_SETS.iter().enumerate() {
                            let mut checked = app.redact_code_sets.contains(&idx);
                            if ui.checkbox(&mut checked, entry.name).clicked() {
                                if checked {
                                    app.redact_code_sets.insert(idx);
                                } else {
                                    app.redact_code_sets.remove(&idx);
                                }
                            }
                        }
                        ui.add_space(SPACE_XS);
                        if ui.button("Detect & Mark").clicked() {
                            let selected: Vec<usize> = app.redact_code_sets.iter().copied().collect();
                            let page = app.tabs.get(app.active).map(|t| t.page).unwrap_or(1);
                            let outcome = app.tabs.get(app.active).and_then(|tab| {
                                let sets: Vec<_> = if selected.is_empty() {
                                    DEFAULT_CODE_SETS.to_vec()
                                } else {
                                    selected
                                        .iter()
                                        .map(|&i| DEFAULT_CODE_SETS[i].clone())
                                        .collect()
                                };
                                mark_code_matches(&tab.doc, page, &sets).ok()
                            });
                            if let Some(marks) = outcome {
                                let count = marks.len();
                                app.redact_marks.extend(marks);
                                app.toast(ctx, format!("{count} code-set match(es) marked"));
                            }
                        }
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button(t("Cancel")).clicked() {
                            cancel = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new(t("Continue")).color(Color32::WHITE))
                                    .fill(pal().accent),
                            )
                            .clicked()
                        {
                            advance = true;
                        }
                    });
                } else {
                    ui.label(
                        RichText::new(format!("{areas} area(s) will be redacted permanently."))
                            .size(BODY)
                            .color(pal().icon_bright),
                    );
                    ui.add_space(SPACE_M);
                    ui.label(
                        RichText::new(
                            "The content under the marked areas is destroyed, not hidden, and cannot be recovered. The rest of each page keeps its selectable text, but a page whose content cannot be sliced cleanly may still be rasterized. Annotations overlapping a marked area are deleted.",
                        )
                        .size(LABEL)
                        .color(pal().muted),
                    );
                    ui.add_space(SPACE_L);
                    ui.checkbox(
                        &mut app.redact_sanitize,
                        "Also remove metadata, XMP, JavaScript and attachments",
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(t("Back")).clicked() {
                            app.redact_phase = RedactPhase::Marking;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new(t("Apply")).color(Color32::WHITE))
                                    .fill(pal().redact_red),
                            )
                            .clicked()
                        {
                            confirm = true;
                        }
                    });
                }
            });
            if cancel || !keep_open {
                self.redact_modal = false;
            }
            if advance {
                self.redact_phase = RedactPhase::Confirming;
            }
            if confirm {
                self.apply_redactions(ctx);
            }
        }

}
