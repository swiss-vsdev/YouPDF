//! Fenêtre « Traitement par lots » et son fil de travail.
//!
//! Le lot ouvre et réécrit des documents entiers : il tourne hors de la trame
//! egui et ne renvoie que des lignes de texte vers l'interface.

use super::*;

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
        let mut choose_dir = false;
        let mut clear = false;
        let mut run = false;
        let files = self.batch_files.len();
        let dir = self
            .batch_dir
            .as_ref()
            .map(|p| p.display().to_string());
        egui::Window::new("Batch processing")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(520.0, 470.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(RichText::new("Files").size(12.0).color(pal().muted));
                ui.horizontal(|ui| {
                    if ui.button("Add PDFs…").clicked() {
                        add_files = true;
                    }
                    if ui.button("Add folder…").clicked() {
                        add_folder = true;
                    }
                    if ui.button("Clear").clicked() {
                        clear = true;
                    }
                    ui.label(RichText::new(format!("{files} file(s)")).size(11.0).color(pal().muted));
                });
                ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                    for path in &self.batch_files {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(RichText::new(name).size(11.0).color(pal().icon_bright));
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Output folder").size(12.0).color(pal().muted));
                ui.horizontal(|ui| {
                    if ui.button("Choose…").clicked() {
                        choose_dir = true;
                    }
                    ui.label(
                        RichText::new(dir.clone().unwrap_or_else(|| "Not chosen".to_owned()))
                            .size(11.0)
                            .color(pal().muted),
                    );
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Operations, applied in this order").size(12.0).color(pal().muted),
                );
                ui.checkbox(&mut self.batch_text, "Export the text (plain text)");
                ui.checkbox(&mut self.batch_images, "Export the pages as PNG (150 dpi)");
                ui.checkbox(&mut self.batch_compress, "Write a compressed copy");
                ui.checkbox(
                    &mut self.batch_protect,
                    "Write a protected copy (current settings)",
                );
                ui.checkbox(
                    &mut self.batch_flatten,
                    "Write a copy with the annotations flattened",
                );
                ui.checkbox(&mut self.batch_combine, "Combine every file into one PDF");
                ui.checkbox(&mut self.batch_print, "Send each file to the system printer");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Convert").size(11.0).color(pal().muted));
                    let current = self.batch_convert;
                    egui::ComboBox::from_id_salt("batch-convert")
                        .selected_text(
                            current
                                .map(|format| format.display_name().to_owned())
                                .unwrap_or_else(|| "None".to_owned()),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), "None").clicked() {
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
                    ui.label(RichText::new("Remove pages").size(11.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_remove_pages)
                            .hint_text("1-2, 5")
                            .desired_width(120.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Watermark").size(11.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_watermark_text)
                            .hint_text("CONFIDENTIAL")
                            .desired_width(160.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Header/footer").size(11.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_header_text)
                            .hint_text("My Company")
                            .desired_width(160.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Bates prefix").size(11.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut self.batch_bates_prefix).desired_width(80.0),
                    );
                    ui.label(RichText::new("start").size(11.0).color(pal().muted));
                    ui.add(egui::DragValue::new(&mut self.batch_bates_start).range(1..=999_999));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Output name").size(11.0).color(pal().muted));
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
                ui.add_space(8.0);
                if ui
                    .add_enabled(
                        files > 0 && self.batch_dir.is_some() && !self.batch_running,
                        egui::Button::new(if self.batch_running {
                            "Running…"
                        } else {
                            "Run"
                        }),
                    )
                    .clicked()
                {
                    run = true;
                }
                if !self.batch_report.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.label(
                            RichText::new(self.batch_report.clone())
                                .size(11.0)
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
                    if !self.batch_files.contains(&path) {
                        self.batch_files.push(path);
                    }
                }
            }
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
                    if !self.batch_files.contains(&path) {
                        self.batch_files.push(path);
                    }
                }
            }
        }
        if clear {
            self.batch_files.clear();
            self.batch_report.clear();
        }
        if choose_dir {
            self.batch_dir = rfd::FileDialog::new().pick_folder();
        }
        if run {
            self.run_batch(ctx);
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
            files: self.batch_files.clone(),
            dir,
            text: self.batch_text,
            images: self.batch_images,
            compress: self.batch_compress,
            protect: self.batch_protect,
            flatten: self.batch_flatten,
            protect_options: self.protect.clone(),
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
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || job.run(&tx));
        self.batch_rx = Some(rx);
        self.batch_running = true;
        self.batch_report.clear();
    }

    /// Consomme les lignes arrivées du fil de travail.
    pub(super) fn pump_batch(&mut self) {
        let Some(rx) = self.batch_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(line) => {
                    self.batch_report.push_str(&line);
                    self.batch_report.push('\n');
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.batch_rx = None;
                    self.batch_running = false;
                    break;
                }
            }
        }
    }
}

/// Lit « 1-3, 7 » en numéros de page bornés au document.
fn parse_page_numbers(text: &str, total: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('-') {
            Some((first, last)) => {
                if let (Ok(first), Ok(last)) = (
                    first.trim().parse::<usize>(),
                    last.trim().parse::<usize>(),
                ) {
                    for number in first..=last.min(total) {
                        out.push(number);
                    }
                }
            }
            None => {
                if let Ok(number) = part.parse::<usize>() {
                    out.push(number);
                }
            }
        }
    }
    out.retain(|number| *number >= 1 && *number <= total);
    out.sort_unstable();
    out.dedup();
    out
}

struct BatchJob {
    files: Vec<PathBuf>,
    dir: PathBuf,
    text: bool,
    images: bool,
    compress: bool,
    protect: bool,
    flatten: bool,
    protect_options: EncryptionOptions,
    convert: Option<OfficeFormat>,
    remove_pages: String,
    watermark_text: String,
    header_text: String,
    bates_prefix: String,
    bates_start: u32,
    combine: bool,
    prefix: String,
    suffix: String,
    print: bool,
}

impl BatchJob {
    /// Traite chaque fichier et envoie une ligne par fichier, puis un bilan.
    ///
    /// Tourne dans un fil de travail : `Document` n'est pas `Send`, il est donc
    /// créé et détruit ici, et seules des `String` repartent vers l'interface.
    fn run(self, tx: &std::sync::mpsc::Sender<String>) {
        let mut done = 0usize;
        let mut failed = 0usize;
        let mut bates_next = self.bates_start.max(1);

        if self.combine && !self.files.is_empty() {
            let out = self.dir.join(format!("{}combined{}", self.prefix, ".pdf"));
            match Document::open(&self.files[0]) {
                Ok(mut merged) => {
                    let mut ok = true;
                    for path in self.files.iter().skip(1) {
                        let after = merged.page_count();
                        if let Err(err) = merged.insert_pages_after(after, path) {
                            ok = false;
                            let _ = tx.send(format!("combine: {err}"));
                            break;
                        }
                    }
                    if ok {
                        match merged.save(&out) {
                            Ok(()) => {
                                let _ = tx.send(format!("combined: {}", out.display()));
                            }
                            Err(err) => {
                                failed += 1;
                                let _ = tx.send(format!("combine: {err}"));
                            }
                        }
                    }
                }
                Err(err) => {
                    failed += 1;
                    let _ = tx.send(format!("combine: {err}"));
                }
            }
        }

        for path in &self.files {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "document".to_owned());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stem = format!("{}{}{}", self.prefix, stem, self.suffix);
            let mut doc = match Document::open(path) {
                Ok(doc) => doc,
                Err(err) => {
                    failed += 1;
                    let _ = tx.send(format!("{name}: open failed — {err}"));
                    continue;
                }
            };
            let mut errors: Vec<String> = Vec::new();
            let mut edited = false;
            if let Some(format) = self.convert {
                let out = self.dir.join(format!("{stem}.{}", format.extension()));
                if let Err(err) = doc.export_office(&out, format) {
                    errors.push(format!("convert: {err}"));
                }
            }
            let removable = parse_page_numbers(&self.remove_pages, doc.page_count());
            if !removable.is_empty() {
                match doc.delete_pages(&removable) {
                    Ok(_) => edited = true,
                    Err(err) => errors.push(format!("remove pages: {err}")),
                }
            }
            if !self.watermark_text.trim().is_empty() {
                let mut watermark = Watermark::default();
                watermark.text = self.watermark_text.clone();
                watermark.tiled = true;
                match doc.add_text_watermark(&watermark) {
                    Ok(_) => edited = true,
                    Err(err) => errors.push(format!("watermark: {err}")),
                }
            }
            if !self.header_text.trim().is_empty() {
                let hf = HeaderFooter {
                    header: BandSlots {
                        center: self.header_text.clone(),
                        ..Default::default()
                    },
                    ..Default::default()
                };
                match doc.add_header_footer(&hf) {
                    Ok(_) => edited = true,
                    Err(err) => errors.push(format!("header/footer: {err}")),
                }
            }
            if !self.bates_prefix.trim().is_empty() {
                let prefix = self.bates_prefix.clone();
                let start = bates_next;
                let bates = Bates {
                    prefix,
                    start,
                    ..Default::default()
                };
                match doc.add_bates(&bates) {
                    Ok(_) => {
                        bates_next = start + doc.page_count() as u32;
                        edited = true;
                    }
                    Err(err) => errors.push(format!("bates: {err}")),
                }
            }
            if self.text {
                let out = self.dir.join(format!("{stem}.txt"));
                match doc.export_text(TextFormat::Plain) {
                    Ok(body) => {
                        if let Err(err) = std::fs::write(&out, body) {
                            errors.push(format!("text: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("text: {err}")),
                }
            }
            if self.images {
                let out = self.dir.join(format!("{stem}-pages"));
                match std::fs::create_dir_all(&out) {
                    Ok(()) => {
                        if let Err(err) = doc.export_images(&out, ImageFormat::Png, 150.0) {
                            errors.push(format!("images: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("images: {err}")),
                }
            }
            if self.compress {
                let out = self.dir.join(format!("{stem}-compressed.pdf"));
                if let Err(err) = doc.compress(&out) {
                    errors.push(format!("compress: {err}"));
                }
            }
            if self.protect {
                let out = self.dir.join(format!("{stem}-protected.pdf"));
                if let Err(err) = doc.save_protected(&out, &self.protect_options) {
                    errors.push(format!("protect: {err}"));
                }
            }
            if self.flatten {
                let out = self.dir.join(format!("{stem}-flat.pdf"));
                match doc.flatten_annotations() {
                    Ok(_) => {
                        if let Err(err) = doc.save(&out) {
                            errors.push(format!("flatten: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("flatten: {err}")),
                }
            }
            if edited && !(self.compress || self.protect || self.flatten) {
                let out = self.dir.join(format!("{stem}-batch.pdf"));
                if let Err(err) = doc.save(&out) {
                    errors.push(format!("save: {err}"));
                }
            }
            if self.print {
                match std::process::Command::new("lp").arg(path).status() {
                    Ok(status) if status.success() => {}
                    Ok(status) => errors.push(format!("print: lp exited with {status}")),
                    Err(err) => errors.push(format!("print: {err}")),
                }
            }
            if errors.is_empty() {
                done += 1;
                let _ = tx.send(format!("{name}: ok"));
            } else {
                failed += 1;
                let _ = tx.send(format!("{name}: {}", errors.join("; ")));
            }
        }
        let _ = tx.send(format!("\n{done} done, {failed} failed"));
    }
}