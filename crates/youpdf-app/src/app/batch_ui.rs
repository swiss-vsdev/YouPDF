//! Fenêtre « Traitement par lots » et son fil de travail.
//!
//! Le lot ouvre et réécrit des documents entiers : il tourne hors de la trame
//! egui et ne renvoie que des lignes de texte vers l'interface.

use super::*;
use super::batch_types::*;
use youpdf_shared::{CAPTION, LABEL, caption, modal_frame, muted, section, t, SPACE_S, SPACE_L, SPACE_M};

impl YouPdfApp {
    pub(super) fn batch_window(&mut self, ctx: &egui::Context) {
        if !self.batch_open {
            return;
        }
        // Le lot tourne dans un fil de travail : on relève ses lignes, et on
        // redemande une trame tant qu'il tourne pour voir la liste avancer.
        self.pump_batch();
        if self.batch_running {
            ctx.request_repaint();
        }
        let mut open = true;
        let mut add_files = false;
        let mut add_folder = false;
        let mut insert_file = false;
        let mut choose_dir = false;
        let mut clear = false;
        let mut run = false;
        let mut cancel = false;
        let files = self.batch_queue.len();
        let dir = self
            .batch_dir
            .as_ref()
            .map(|p| p.display().to_string());
        egui::Window::new(t("Batch processing"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(520.0, 470.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                ui.label(section(t("Queue")));
                ui.horizontal(|ui| {
                    if ui.button(t("Add PDFs…")).clicked() {
                        add_files = true;
                    }
                    if ui.button(t("Add folder…")).clicked() {
                        add_folder = true;
                    }
                    if ui.add_enabled(!self.batch_running, egui::Button::new("Clear")).clicked() {
                        clear = true;
                    }
                    ui.label(muted(format!("{files} file(s)"), LABEL));
                });
                ui.add_space(SPACE_S);
                self.batch_table(ui);
                ui.add_space(SPACE_M);
                ui.label(section(t("Output folder")));
                ui.horizontal(|ui| {
                    if ui.button(t("Choose…")).clicked() {
                        choose_dir = true;
                    }
                    ui.label(
                        RichText::new(dir.clone().unwrap_or_else(|| "Not chosen".to_owned()))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                });
                ui.add_space(SPACE_L);
                ui.separator();
                ui.add_space(SPACE_M);
                ui.label(
                    section(t("Operations, applied in this order")),
                );
                ui.checkbox(&mut self.batch_text, t("Export the text (plain text)"));
                ui.checkbox(&mut self.batch_images, t("Export the pages as PNG (150 dpi)"));
                ui.checkbox(&mut self.batch_compress, t("Write a compressed copy"));
                ui.checkbox(
                    &mut self.batch_protect,
                    "Write a protected copy (current settings)",
                );
                ui.checkbox(
                    &mut self.batch_flatten,
                    "Write a copy with the annotations flattened",
                );
                ui.checkbox(&mut self.batch_combine, t("Combine every file into one PDF"));
                ui.checkbox(&mut self.batch_print, t("Send each file to the system printer"));
                ui.checkbox(&mut self.batch_ocr, t("Recognise text (OCR, searchable)"));
                if self.batch_ocr {
                    ui.horizontal(|ui| {
                        ui.label(section(t("OCR language")));
                        let mut picked: Option<String> = None;
                        egui::ComboBox::from_id_salt("batch-ocr-lang")
                            .selected_text(language_name(&self.ocr_language))
                            .show_ui(ui, |ui| {
                                for lang in LANGUAGES {
                                    if ui
                                        .selectable_label(
                                            self.ocr_language == lang.code,
                                            lang.name,
                                        )
                                        .clicked()
                                    {
                                        picked = Some(lang.code.to_string());
                                    }
                                }
                            });
                        if let Some(code) = picked {
                            self.ocr_language = code;
                        }
                    });
                }
                ui.checkbox(
                    &mut self.batch_background,
                    "Apply the current page background",
                );
                ui.horizontal(|ui| {
                    if ui.button(t("Insert pages from…")).clicked() {
                        insert_file = true;
                    }
                    ui.label(
                        RichText::new(
                            self.batch_insert
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "No file chosen".to_owned()),
                        )
                        .size(LABEL)
                        .color(pal().muted),
                    );
                    ui.label(muted(t("at"), LABEL));
                    let mut at_page = self.batch_insert_at.min(9999) as u32;
                    ui.add(egui::DragValue::new(&mut at_page).range(0..=9999).prefix("after "));
                    self.batch_insert_at = if at_page == 0 {
                        0
                    } else {
                        at_page as usize
                    };
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Convert"), LABEL));
                    let current = self.batch_convert;
                    egui::ComboBox::from_id_salt("batch-convert")
                        .selected_text(
                            current
                                .map(|format| format.display_name().to_owned())
                                .unwrap_or_else(|| "None".to_owned()),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), t("None")).clicked() {
                                self.batch_convert = None;
                            }
                            for format in OfficeFormat::ALL {
                                if ui
                                    .selectable_label(current == Some(format), format.display_name())
                                    .clicked()
                                {
                                    self.batch_convert = Some(format);
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Remove pages"), LABEL));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_remove_pages)
                            .hint_text("1-2, 5")
                            .desired_width(120.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Watermark"), LABEL));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_watermark_text)
                            .hint_text("CONFIDENTIAL")
                            .desired_width(160.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Header/footer"), LABEL));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_header_text)
                            .hint_text("My Company")
                            .desired_width(160.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Bates prefix"), LABEL));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_bates_prefix).desired_width(80.0),
                    );
                    ui.label(muted(t("start"), LABEL));
                    ui.add(egui::DragValue::new(&mut self.batch_bates_start).range(1..=999_999));
                });
                ui.horizontal(|ui| {
                    ui.label(muted(t("Output name"), LABEL));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_prefix)
                            .hint_text("prefix")
                            .desired_width(90.0),
                    );
                    ui.add(
                        TextEdit::singleline(&mut self.batch_suffix)
                            .hint_text("suffix")
                            .desired_width(90.0),
                    );
                });
                ui.add_space(SPACE_L);
                ui.horizontal(|ui| {
                    if self.batch_running {
                        if ui.button(t("Cancel")).clicked() {
                            cancel = true;
                        }
                        ui.spinner();
                        ui.label(
                            RichText::new(self.batch_progress_label())
                                .size(LABEL)
                                .color(pal().icon_bright),
                        );
                    } else if ui
                        .add_enabled(
                            files > 0 && self.batch_dir.is_some(),
                            egui::Button::new("Run"),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                });
                if self.batch_running {
                    let fraction = self.batch_fraction();
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!("{:.0}%", fraction * 100.0))
                            .desired_width(280.0),
                    );
                    if let Some(eta) = self.batch_eta {
                        ui.label(
                            RichText::new(format!("About {} left", human_duration(eta)))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                    }
                }
                if !self.batch_report.is_empty() {
                    ui.add_space(SPACE_L);
                    ui.separator();
                    ui.add_space(SPACE_M);
                    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.label(
                            RichText::new(self.batch_report.clone())
                                .size(LABEL)
                                .color(pal().icon_bright),
                        );
                    });
                }
            });

        if add_files {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_files()
            {
                for path in paths {
                    self.push_batch_file(path);
                }
            }
        }
        if insert_file {
            self.batch_insert = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file();
        }
        if add_folder {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let mut found: Vec<PathBuf> = std::fs::read_dir(&folder)
                    .map(|entries| {
                        entries
                            .filter_map(|entry| entry.ok())
                            .map(|entry| entry.path())
                            .filter(|path| {
                                path.extension()
                                    .map(|ext| ext.eq_ignore_ascii_case("pdf"))
                                    .unwrap_or(false)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                found.sort();
                for path in found {
                    self.push_batch_file(path);
                }
            }
        }
        if clear {
            self.batch_queue.clear();
            self.batch_report.clear();
        }
        if choose_dir {
            self.batch_dir = rfd::FileDialog::new().pick_folder();
        }
        if run {
            self.run_batch(ctx);
        }
        if cancel {
            self.cancel_batch(ctx);
        }
        if !open {
            self.batch_open = false;
        }
    }

    /// Lance le lot dans un fil de travail.
    ///
    /// Chaque fichier est traité en séquence : les opérations écrivent des
    /// copies dans le dossier de sortie, l'original n'est jamais touché. Le lot
    /// ouvre et réécrit des documents entiers ; le faire dans la trame d'egui
    /// gelait l'interface pour toute sa durée. Le fil envoie une ligne par
    /// fichier, que la fenêtre consomme au fil de l'eau.
    pub(super) fn run_batch(&mut self, ctx: &egui::Context) {
        if self.batch_running {
            return;
        }
        let Some(dir) = self.batch_dir.clone() else {
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.toast(ctx, format!("Batch failed: {err}"));
            return;
        }
        let job = BatchJob {
            files: self
                .batch_queue
                .iter()
                .map(|entry| entry.path.clone())
                .collect(),
            dir,
            text: self.batch_text,
            images: self.batch_images,
            compress: self.batch_compress,
            protect: self.batch_protect,
            flatten: self.batch_flatten,
            protect_options: self.protect.clone(),
            insert: self.batch_insert.clone(),
            insert_at: self.batch_insert_at,
            ocr: self.batch_ocr,
            ocr_language: self.ocr_language.clone(),
            background: self.batch_background.then(|| self.background.clone()),
            convert: self.batch_convert,
            remove_pages: self.batch_remove_pages.clone(),
            watermark_text: self.batch_watermark_text.clone(),
            header_text: self.batch_header_text.clone(),
            bates_prefix: self.batch_bates_prefix.clone(),
            bates_start: self.batch_bates_start,
            combine: self.batch_combine,
            prefix: self.batch_prefix.clone(),
            suffix: self.batch_suffix.clone(),
            print: self.batch_print,
        };
        for entry in &mut self.batch_queue {
            entry.reset();
        }
        self.batch_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel = self.batch_cancel.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || job.run(&tx, &cancel));
        self.batch_rx = Some(rx);
        self.batch_running = true;
        self.batch_report.clear();
        self.batch_started = Some(std::time::Instant::now());
        self.batch_eta = None;
    }

    /// Demande l'arrêt du lot : le fil s'arrête après le fichier en cours.
    fn cancel_batch(&mut self, ctx: &egui::Context) {
        if !self.batch_running {
            return;
        }
        self.batch_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.toast(ctx, "Stopping after the current file".to_string());
    }

    /// Ajoute un fichier à la file, sans doublon.
    pub(super) fn push_batch_file(&mut self, path: PathBuf) {
        if self.batch_queue.iter().any(|entry| entry.path == path) {
            return;
        }
        self.batch_queue.push(BatchEntry::new(path));
    }

    /// Consomme les événements arrivés du fil de travail.
    pub(super) fn pump_batch(&mut self) {
        let Some(rx) = self.batch_rx.as_ref() else {
            return;
        };
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    match event {
                        BatchEvent::Started(index) => {
                            if let Some(entry) = self.batch_queue.get_mut(index) {
                                entry.status = BatchStatus::Running;
                            }
                        }
                        BatchEvent::Finished { index, outcome } => {
                            if let Some(entry) = self.batch_queue.get_mut(index) {
                                entry.status = match &outcome {
                                    Ok(()) => BatchStatus::Done,
                                    Err(_) => BatchStatus::Failed,
                                };
                                entry.detail = outcome.err();
                            }
                            self.batch_eta = self.estimate_eta();
                        }
                        BatchEvent::Note(line) => {
                            self.batch_report.push_str(&line);
                            self.batch_report.push('\n');
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.batch_rx = None;
            self.batch_running = false;
            self.batch_started = None;
            self.batch_eta = None;
            // Un fichier interrompu n'a pas d'état final : on le dit plutôt
            // que de le laisser tourner indéfiniment.
            for entry in &mut self.batch_queue {
                if entry.status == BatchStatus::Running {
                    entry.status = BatchStatus::Failed;
                    entry.detail = Some("cancelled".to_string());
                }
            }
        }
    }

    /// Part du lot déjà traitée, entre 0 et 1.
    fn batch_fraction(&self) -> f32 {
        if self.batch_queue.is_empty() {
            return 0.0;
        }
        let settled = self
            .batch_queue
            .iter()
            .filter(|entry| entry.status.is_settled())
            .count();
        settled as f32 / self.batch_queue.len() as f32
    }

    /// Temps restant estimé, d'après le rythme observé depuis le départ.
    fn estimate_eta(&self) -> Option<std::time::Duration> {
        let started = self.batch_started?;
        let settled = self
            .batch_queue
            .iter()
            .filter(|entry| entry.status.is_settled())
            .count();
        if settled == 0 {
            return None;
        }
        let elapsed = started.elapsed();
        let per_file = elapsed / settled as u32;
        let remaining = self.batch_queue.len() - settled;
        Some(per_file * remaining as u32)
    }

    /// « 3 of 10 done · 1 failed ».
    fn batch_progress_label(&self) -> String {
        let total = self.batch_queue.len();
        let done = self
            .batch_queue
            .iter()
            .filter(|entry| entry.status == BatchStatus::Done)
            .count();
        let failed = self
            .batch_queue
            .iter()
            .filter(|entry| entry.status == BatchStatus::Failed)
            .count();
        if failed == 0 {
            format!("{done} of {total} done")
        } else {
            format!("{done} of {total} done · {failed} failed")
        }
    }

    /// Tableau de la file : index, nom, pages, taille, état.
    fn batch_table(&mut self, ui: &mut egui::Ui) {
        use egui::{Grid, Label};
        ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
            Grid::new("batch-table")
                .num_columns(5)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (heading, _) in [
                        ("#", 0.0),
                        ("Name", 0.0),
                        ("Pages", 0.0),
                        ("Size", 0.0),
                        ("Status", 0.0),
                    ] {
                        ui.add(Label::new(
                            caption(heading),
                        ));
                    }
                    ui.end_row();
                    for (index, entry) in self.batch_queue.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("{}", index + 1))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        ui.label(
                            RichText::new(entry.name()).size(LABEL).color(pal().icon_bright),
                        );
                        let pages = entry
                            .pages
                            .map(|count| count.to_string())
                            .unwrap_or_else(|| "—".to_string());
                        ui.label(caption(pages));
                        ui.label(
                            RichText::new(human_bytes(entry.size))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        let (text, colour) = entry.status.label();
                        let mut label = RichText::new(text).size(CAPTION).color(colour);
                        if let Some(detail) = &entry.detail {
                            label = label.strong();
                            ui.label(label).on_hover_text(detail);
                        } else {
                            ui.label(label);
                        }
                        ui.end_row();
                    }
                });
        });
    }
}
