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

// Suite du panel attachments_ui (extraite de `app.rs`).

use super::*;

impl YouPdfApp {

        /// Embarque un fichier choisi par l'utilisateur.
        pub(super) fn add_attachment(&mut self, ctx: &egui::Context) {
            let Some(path) = rfd::FileDialog::new().set_title("Attach a file").pick_file() else {
                return;
            };
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    self.toast(ctx, format!("Attach failed: {err}"));
                    return;
                }
            };
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("attachment")
                .to_owned();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_attachment(&name, None, &bytes));
            match result {
                Some(Ok(())) => {
                    self.mark_document_changed();
                    self.ensure_structure();
                    self.toast(ctx, format!("Attached {name}"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Attach failed: {err}")),
                None => {}
            }
        }

        /// Ouvre une pièce jointe dans l'application par défaut du système.
        ///
        /// Le fichier est extrait sous un nom assaini, dans un dossier temporaire
        /// privé et propre à cette extraction : ni un nom hostile ni un lien
        /// symbolique planté à l'avance ne doivent sortir de ce dossier.
        pub(super) fn open_attachment(&mut self, index: usize, ctx: &egui::Context) {
            let Some((bytes, name)) = self.tabs.get(self.active).and_then(|tab| {
                let name = tab
                    .attachments
                    .get(index)
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| format!("attachment-{index}"));
                tab.doc.attachment_bytes(index).ok().map(|bytes| (bytes, name))
            }) else {
                self.toast(ctx, "Attachment failed");
                return;
            };
            let file_name = safe_attachment_name(&name, index);
            let path = match stage_attachment(&bytes, &file_name) {
                Ok(path) => path,
                Err(err) => {
                    self.toast(ctx, format!("Open failed: {err}"));
                    return;
                }
            };
            // Les extractions d'hier ne servent plus à rien : elles partent au
            // prochain usage, faute de pouvoir être retirées dès l'ouverture
            // (l'application externe peut encore lire le fichier).
            let _ = prune_attachment_staging(ATTACHMENT_STAGING_MAX_AGE);
            match self.open_with_system(&path) {
                Ok(()) => self.toast(ctx, format!("Opened {}", path.display())),
                Err(err) => self.toast(ctx, format!("Open failed: {err}")),
            }
        }

        /// Écrit une pièce jointe à l'emplacement choisi par l'utilisateur.
        pub(super) fn save_attachment(&mut self, index: usize, ctx: &egui::Context) {
            let Some((bytes, name)) = self.tabs.get(self.active).and_then(|tab| {
                let name = tab
                    .attachments
                    .get(index)
                    .map(|item| item.name.clone())
                    .unwrap_or_else(|| format!("attachment-{index}"));
                tab.doc.attachment_bytes(index).ok().map(|bytes| (bytes, name))
            }) else {
                self.toast(ctx, "Attachment failed");
                return;
            };
            let Some(target) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
                return;
            };
            match std::fs::write(&target, bytes) {
                Ok(()) => self.toast(ctx, format!("Saved {}", target.display())),
                Err(err) => self.toast(ctx, format!("Write failed: {err}")),
            }
        }

        /// Retire une pièce jointe du document.
        pub(super) fn delete_attachment(&mut self, index: usize, ctx: &egui::Context) {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.remove_attachment(index));
            match result {
                Some(Ok(())) => {
                    self.mark_document_changed();
                    self.ensure_structure();
                    self.toast(ctx, "Attachment removed");
                }
                Some(Err(err)) => self.toast(ctx, format!("Delete failed: {err}")),
                None => {}
            }
        }

        /// Demande le mot de passe d'un fichier protégé pour le déverrouiller.
        pub(super) fn remove_protection(&mut self, ctx: &egui::Context) {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            else {
                return;
            };
            if !Document::file_is_encrypted(&path) {
                self.toast(ctx, "This file is not protected");
                return;
            }
            self.password_prompt = Some(PasswordPrompt {
                path,
                purpose: PasswordPurpose::RemoveProtection,
                value: String::new(),
                error: None,
            });
        }

}
