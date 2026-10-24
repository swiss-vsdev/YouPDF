//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel password_ui).


use super::*;

impl YouPdfApp {

        /// Fenêtre d'invite de mot de passe (ouverture ou déprotection).
        pub(super) fn password_window(&mut self, ctx: &egui::Context) {
            if self.password_prompt.is_none() {
                return;
            }
            let mut open = true;
            let mut submit = false;
            let mut cancel = false;
            let (title, name, purpose) = {
                let prompt = self.password_prompt.as_ref().unwrap();
                let title = match prompt.purpose {
                    PasswordPurpose::Open => "Password required",
                    PasswordPurpose::RemoveProtection => "Remove protection",
                };
                let name = prompt
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "This file".to_owned());
                (title, name, prompt.purpose)
            };
            self.show_modal(ctx, "modal-password", title, 340.0, &mut open, |app, ui| {
                let prompt = app.password_prompt.as_mut().unwrap();
                ui.label(
                    RichText::new(format!("{name} is protected. Enter its password."))

                        .color(pal().icon_bright),
                );
                ui.add_space(SPACE_M);
                let resp = ui.add(
                    TextEdit::singleline(&mut prompt.value)
                        .password(true)
                        .desired_width(300.0)
                        .hint_text("Password"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                if let Some(err) = &prompt.error {
                    ui.add_space(SPACE_S);
                    ui.label(RichText::new(err).size(LABEL).color(pal().redact_red));
                }
                ui.add_space(SPACE_L);
                ui.horizontal(|ui| {
                    if ui.button(t("OK")).clicked() {
                        submit = true;
                    }
                    if ui.button(t("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
            if !open {
                cancel = true;
            }
            if cancel {
                self.password_prompt = None;
                return;
            }
            if !submit {
                return;
            }
            let prompt = self.password_prompt.take().unwrap();
            match Document::open_protected(&prompt.path, &prompt.value) {
                Ok(doc) => match purpose {
                    PasswordPurpose::Open => self.push_document(doc, Some(prompt.path)),
                    PasswordPurpose::RemoveProtection => {
                        let stem = prompt
                            .path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "document".to_owned());
                        if let Some(out) = rfd::FileDialog::new()
                            .add_filter("PDF", &["pdf"])
                            .set_file_name(format!("{stem}-unprotected.pdf"))
                            .save_file()
                        {
                            match doc.save_decrypted(&out) {
                                Ok(()) => {
                                    let name = out
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default();
                                    self.toast(ctx, format!("Unprotected copy written to {name}"));
                                }
                                Err(err) => {
                                    self.toast(ctx, format!("Remove protection failed: {err}"));
                                }
                            }
                        }
                    }
                },
                Err(err) => {
                    let mut prompt = prompt;
                    prompt.value.clear();
                    prompt.error = Some(format!("{err}"));
                    self.password_prompt = Some(prompt);
                }
            }
        }

}
