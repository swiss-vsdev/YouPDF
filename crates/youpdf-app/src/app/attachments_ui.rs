//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel attachments_ui).


use super::*;

impl YouPdfApp {

        /// Panneau Formulaires : lecture et remplissage des champs.
        /// Panneau Pièces jointes : ajout, ouverture, extraction et retrait.
        pub(super) fn attachments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            self.ensure_structure();
            let count = self.tabs.get(self.active).map_or(0, |tab| tab.attachments.len());

            let mut add = false;
            ui.horizontal(|ui| {
                ui.add_space(SPACE_S);
                if ui.button(RichText::new(t("Add Attachment")).size(LABEL)).clicked() {
                    add = true;
                }
                ui.label(
                    RichText::new(format!("{count} file(s)"))
                        .size(CAPTION)
                        .color(pal().muted),
                );
            });
            ui.add_space(SPACE_S);
            if add {
                self.add_attachment(ctx);
                return;
            }

            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            if tab.attachments.is_empty() {
                panel_hint(ui, "No attachments", "Use Add Attachment to embed a file");
                return;
            }
            let rows: Vec<(String, Option<usize>, Option<String>)> = tab
                .attachments
                .iter()
                .map(|item| (item.name.clone(), item.size, item.description.clone()))
                .collect();

            let panel_w = self.panel_w;
            let mut action: Option<AttachmentAction> = None;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(SPACE_M);
                    for (idx, (name, size, description)) in rows.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(panel_w - 24.0, 44.0), Sense::click());
                        if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
                        }
                        let icon_rect = Rect::from_center_size(
                            pos2(rect.min.x + 16.0, rect.center().y),
                            vec2(15.0, 15.0),
                        );
                        icon_attach(ui.painter(), icon_rect, pal().icon_bright);
                        ui.painter().text(
                            pos2(rect.min.x + 30.0, rect.min.y + 13.0),
                            Align2::LEFT_CENTER,
                            shorten(name, 24),
                            FontId::proportional(BODY),
                            pal().icon_bright,
                        );
                        let mut detail = size
                            .map(format_size)
                            .unwrap_or_else(|| "size unknown".to_owned());
                        if let Some(description) = description {
                            detail = format!("{detail} · {}", shorten(description, 16));
                        }
                        ui.painter().text(
                            pos2(rect.min.x + 30.0, rect.min.y + 30.0),
                            Align2::LEFT_CENTER,
                            detail,
                            FontId::proportional(CAPTION),
                            pal().muted,
                        );
                        let save_rect = Rect::from_center_size(
                            pos2(rect.max.x - 18.0, rect.center().y),
                            vec2(22.0, 22.0),
                        );
                        let save_resp =
                            ui.interact(save_rect, Id::new(("attach-save", idx)), Sense::click());
                        icon_save(
                            ui.painter(),
                            save_rect.shrink(3.0),
                            if save_resp.hovered() { pal().icon_bright } else { pal().icon },
                        );
                        if save_resp.clicked() {
                            action = Some(AttachmentAction::SaveAs(idx));
                        } else if resp.double_clicked() {
                            action = Some(AttachmentAction::Open(idx));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(t("Open")).clicked() {
                                action = Some(AttachmentAction::Open(idx));
                                ui.close_menu();
                            }
                            if ui.button(t("Save As…")).clicked() {
                                action = Some(AttachmentAction::SaveAs(idx));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(t("Delete")).clicked() {
                                action = Some(AttachmentAction::Delete(idx));
                                ui.close_menu();
                            }
                        });
                        ui.add_space(SPACE_S);
                    }
                });

            match action {
                Some(AttachmentAction::Open(idx)) => self.open_attachment(idx, ctx),
                Some(AttachmentAction::SaveAs(idx)) => self.save_attachment(idx, ctx),
                Some(AttachmentAction::Delete(idx)) => self.delete_attachment(idx, ctx),
                None => {}
            }
        }

}
