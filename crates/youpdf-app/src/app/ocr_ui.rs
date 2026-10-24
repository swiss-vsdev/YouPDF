//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel ocr_ui).


use super::*;

impl YouPdfApp {

        /// Fenêtre « OCR » : rend le document consultable par Tesseract.
        pub(super) fn ocr_window(&mut self, ctx: &egui::Context) {
            if !self.ocr_open {
                return;
            }
            let mut open = true;
            let mut run = false;
            // Tesseract n'est interrogé qu'une fois : `available_languages` lance
            // un processus, et le refaire à chaque trame ferait clignoter la
            // fenêtre tout en lançant un `tesseract --list-langs` par image.
            if !self.ocr_probed {
                let engine = TesseractEngine::detect();
                self.ocr_installed = engine.display_name().contains("système");
                self.ocr_languages = engine.available_languages().unwrap_or_default();
                self.ocr_probed = true;
            }
            let installed = self.ocr_installed;
            let languages = self.ocr_languages.clone();
            self.show_modal(ctx, "modal-ocr", "OCR", 430.0, &mut open, |app, ui| {
                ui.label(
                    RichText::new(if installed {
                        "Tesseract (system)"
                    } else {
                        "Tesseract (not installed)"
                    })

                    .color(if installed { pal().icon_bright } else { pal().redact_red }),
                );
                ui.add_space(SPACE_M);
                ui.horizontal(|ui| {
                    ui.label(section(t("Language")));
                    ui.add(TextEdit::singleline(&mut app.ocr_language).desired_width(90.0));
                    if !languages.is_empty() {
                        let mut picked: Option<String> = None;
                        egui::ComboBox::from_id_salt("ocr-lang")
                            .selected_text(app.ocr_language.clone())
                            .show_ui(ui, |ui| {
                                for lang in languages.iter().take(300) {
                                    if ui
                                        .selectable_label(app.ocr_language == *lang, lang)
                                        .clicked()
                                    {
                                        picked = Some(lang.clone());
                                    }
                                }
                            });
                        if let Some(lang) = picked {
                            app.ocr_language = lang;
                        }
                    }
                });
                // Langues supplémentaires : tesseract accepte `eng+fra` pour un
                // document qui mélange les deux. Le catalogue donne le nom lisible
                // de chaque langue, même quand le paquet n'est pas installé.
                if !app.ocr_extra_languages.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(section(t("Also")));
                        for code in app.ocr_extra_languages.clone() {
                            ui.label(
                                RichText::new(language_name(&code))

                                    .color(pal().icon_bright),
                            );
                        }
                    });
                }
                ui.add_space(SPACE_S);
                egui::CollapsingHeader::new("Add another language")
                    .default_open(!app.ocr_extra_languages.is_empty())
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                            for lang in LANGUAGES {
                                let code = lang.code.to_string();
                                // La langue principale ne peut pas être aussi
                                // une langue additionnelle.
                                if app.ocr_language == lang.code {
                                    continue;
                                }
                                let mut checked = app.ocr_extra_languages.contains(&code);
                                if ui
                                    .add(egui::Checkbox::new(&mut checked, lang.name))
                                    .on_hover_text(lang.code)
                                    .changed()
                                {
                                    if checked {
                                        app.ocr_extra_languages.push(code);
                                    } else {
                                        app.ocr_extra_languages.retain(|c| c != &code);
                                    }
                                }
                            }
                        });
                    });
                ui.add_space(SPACE_M);
                ui.label(section(t("Output")));
                for &(mode, label, detail) in OcrMode::ALL {
                    if ui
                        .selectable_label(app.ocr_mode == mode, format!("{label} — {detail}"))
                        .clicked()
                    {
                        app.ocr_mode = mode;
                    }
                }
                ui.add_space(SPACE_L);
                ui.label(
                    RichText::new(
                        "Recognition rewrites the document: each page becomes an image plus \
                         an invisible text layer. It runs on the interface thread and can \
                         take a while on a long scan.",
                    )
                    .size(LABEL)
                    .color(pal().muted),
                );
                ui.add_space(SPACE_M);
                if ui
                    .add_enabled(installed, egui::Button::new("Recognise"))
                    .clicked()
                {
                    run = true;
                }
                if let Some(report) = &app.ocr_report {
                    ui.add_space(SPACE_M);
                    ui.label(RichText::new(report).size(LABEL).color(pal().icon_bright));
                }
            });

            if run {
                // Une seule langue : le code tel quel. Plusieurs langues :
                // tesseract les attend séparées par `+`.
                let mut codes = vec![self.ocr_language.clone()];
                codes.extend(self.ocr_extra_languages.iter().cloned());
                let language = match combine_languages(&codes) {
                    Ok(language) => language,
                    Err(err) => {
                        self.toast(ctx, format!("OCR failed: {err}"));
                        return;
                    }
                };
                let mode = self.ocr_mode;
                let result = {
                    let engine = TesseractEngine::detect();
                    self.tabs.get(self.active).map(|tab| {
                        make_ocr(&tab.doc, &*self.renderer, &engine, &language, mode)
                    })
                };
                match result {
                    Some(Ok(doc)) => {
                        let pages = doc.page_count();
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            tab.doc = doc;
                        }
                        self.mark_document_changed();
                        let report = format!("{pages} page(s) recognised");
                        self.ocr_report = Some(report.clone());
                        self.toast(ctx, format!("OCR done: {report}"));
                    }
                    Some(Err(err)) => {
                        let message = format!("{err}");
                        self.ocr_report = Some(message.clone());
                        self.toast(ctx, format!("OCR failed: {message}"));
                    }
                    None => self.toast(ctx, "Open a document first"),
                }
            }
            if !open {
                self.ocr_open = false;
            }
        }

}
