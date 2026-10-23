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
        egui::Window::new("Batch processing")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(520.0, 470.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(620.0);
                ui.label(RichText::new("Queue").size(12.0).color(pal().muted));
                ui.horizontal(|ui| {
                    if ui.button("Add PDFs…").clicked() {
                        add_files = true;
                    }
                    if ui.button("Add folder…").clicked() {
                        add_folder = true;
                    }
                    if ui.add_enabled(!self.batch_running, egui::Button::new("Clear")).clicked() {
                        clear = true;
                    }
                    ui.label(RichText::new(format!("{files} file(s)")).size(11.0).color(pal().muted));
                });
                ui.add_space(4.0);
                self.batch_table(ui);
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
                ui.checkbox(&mut self.batch_ocr, "Recognise text (OCR, searchable)");
                if self.batch_ocr {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("OCR language").size(12.0).color(pal().muted));
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
                    if ui.button("Insert pages from…").clicked() {
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
                        .size(11.0)
                        .color(pal().muted),
                    );
                    ui.label(RichText::new("at").size(11.0).color(pal().muted));
                    let mut at_page = self.batch_insert_at.min(9999) as u32;
                    ui.add(egui::DragValue::new(&mut at_page).range(0..=9999).prefix("after "));
                    self.batch_insert_at = if at_page == 0 {
                        0
                    } else {
                        at_page as usize
                    };
                });
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
                ui.horizontal(|ui| {
                    if self.batch_running {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        ui.spinner();
                        ui.label(
                            RichText::new(self.batch_progress_label())
                                .size(11.0)
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
                                .size(10.5)
                                .color(pal().muted),
                        );
                    }
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
                            RichText::new(heading).size(10.5).color(pal().muted),
                        ));
                    }
                    ui.end_row();
                    for (index, entry) in self.batch_queue.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("{}", index + 1))
                                .size(10.5)
                                .color(pal().muted),
                        );
                        ui.label(
                            RichText::new(entry.name()).size(11.0).color(pal().icon_bright),
                        );
                        let pages = entry
                            .pages
                            .map(|count| count.to_string())
                            .unwrap_or_else(|| "—".to_string());
                        ui.label(RichText::new(pages).size(10.5).color(pal().muted));
                        ui.label(
                            RichText::new(human_bytes(entry.size))
                                .size(10.5)
                                .color(pal().muted),
                        );
                        let (text, colour) = entry.status.label();
                        let mut label = RichText::new(text).size(10.5).color(colour);
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

/// Un fichier de la file, avec ce que le lot en a fait.
pub(super) struct BatchEntry {
    path: PathBuf,
    /// Nombre de pages, lu une fois à l'ajout.
    pages: Option<usize>,
    /// Taille du fichier en octets.
    size: u64,
    status: BatchStatus,
    /// Message d'échec, affiché au survol.
    detail: Option<String>,
}

impl BatchEntry {
    fn new(path: PathBuf) -> Self {
        let meta = path.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let pages = Document::open(&path).ok().map(|doc| doc.page_count());
        Self {
            path,
            pages,
            size,
            status: BatchStatus::Pending,
            detail: None,
        }
    }

    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    fn reset(&mut self) {
        self.status = BatchStatus::Pending;
        self.detail = None;
    }
}

/// État d'un fichier dans la file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BatchStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl BatchStatus {
    fn is_settled(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }

    fn label(self) -> (&'static str, Color32) {
        match self {
            Self::Pending => ("Pending", pal().muted),
            Self::Running => ("Processing", pal().icon_bright),
            Self::Done => ("Done", DONE_GREEN),
            Self::Failed => ("Error", FAILED_RED),
        }
    }
}

/// Événement envoyé par le fil du lot vers l'interface.
pub(super) enum BatchEvent {
    /// Le fichier d'index `n` commence.
    Started(usize),
    /// Le fichier d'index `n` est fini, avec ou sans erreur.
    Finished { index: usize, outcome: Result<(), String> },
    /// Ligne de journal (combine, bilan).
    Note(String),
}

/// Vert des lignes « Done ».
const DONE_GREEN: Color32 = Color32::from_rgb(52, 148, 92);
/// Rouge des lignes « Error ».
const FAILED_RED: Color32 = Color32::from_rgb(196, 62, 62);

/// « 1.4 MB », « 812 B ».
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// « 2 min 10 s », « 8 s ».
fn human_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min {} s", seconds / 60, seconds % 60)
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

/// Un lot à traiter : les fichiers, le dossier de sortie et les opérations
/// cochées.
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
    insert: Option<PathBuf>,
    insert_at: usize,
    ocr: bool,
    ocr_language: String,
    background: Option<Background>,
}

impl BatchJob {
    /// Traite chaque fichier et envoie une ligne par fichier, puis un bilan.
    ///
    /// Tourne dans un fil de travail : `Document` n'est pas `Send`, il est donc
    /// créé et détruit ici, et seules des `String` repartent vers l'interface.
    fn run(
        self,
        tx: &std::sync::mpsc::Sender<BatchEvent>,
        cancel: &std::sync::atomic::AtomicBool,
    ) {
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
                            let _ = tx.send(BatchEvent::Note(format!("combine: {err}")));
                            break;
                        }
                    }
                    if ok {
                        match merged.save(&out) {
                            Ok(()) => {
                                let _ = tx.send(BatchEvent::Note(format!(
                                    "combined: {}",
                                    out.display()
                                )));
                            }
                            Err(err) => {
                                failed += 1;
                                let _ = tx.send(BatchEvent::Note(format!("combine: {err}")));
                            }
                        }
                    }
                }
                Err(err) => {
                    failed += 1;
                    let _ = tx.send(BatchEvent::Note(format!("combine: {err}")));
                }
            }
        }

        for (index, path) in self.files.iter().enumerate() {
            // L'arrêt est demandé après le fichier en cours : un document à
            // moitié traité laisserait une sortie incohérente sur le disque.
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = tx.send(BatchEvent::Finished {
                    index,
                    outcome: Err("cancelled".to_string()),
                });
                continue;
            }
            let _ = tx.send(BatchEvent::Started(index));
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "document".to_owned());
            let stem = format!("{}{}{}", self.prefix, stem, self.suffix);
            let mut doc = match Document::open(path) {
                Ok(doc) => doc,
                Err(err) => {
                    failed += 1;
                    let _ = tx.send(BatchEvent::Finished {
                        index,
                        outcome: Err(format!("open failed — {err}")),
                    });
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
            if let Some(source) = &self.insert {
                let after = if self.insert_at == 0 {
                    0
                } else {
                    self.insert_at.min(doc.page_count())
                };
                match doc.insert_pages_after(after, source) {
                    Ok(_) => edited = true,
                    Err(err) => errors.push(format!("insert: {err}")),
                }
            }
            if let Some(background) = &self.background {
                match doc.add_background(background) {
                    Ok(_) => edited = true,
                    Err(err) => errors.push(format!("background: {err}")),
                }
            }
            if self.ocr {
                match PdfiumRenderer::load() {
                    Ok(renderer) => {
                        let engine = TesseractEngine::detect();
                        match make_ocr(&doc, &renderer, &engine, &self.ocr_language, OcrMode::Searchable) {
                            Ok(recognised) => {
                                doc = recognised;
                                edited = true;
                            }
                            Err(err) => errors.push(format!("OCR: {err}")),
                        }
                    }
                    Err(err) => errors.push(format!("OCR: {err}")),
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
                let watermark = Watermark {
                    text: self.watermark_text.clone(),
                    tiled: true,
                    ..Default::default()
                };
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
                let _ = tx.send(BatchEvent::Finished {
                    index,
                    outcome: Ok(()),
                });
            } else {
                failed += 1;
                let _ = tx.send(BatchEvent::Finished {
                    index,
                    outcome: Err(errors.join("; ")),
                });
            }
        }
        let _ = tx.send(BatchEvent::Note(format!(
            "\n{done} done, {failed} failed"
        )));
    }
}
#[cfg(test)]
mod tests {
    use super::{human_bytes, human_duration, BatchStatus};
    use std::time::Duration;

    #[test]
    fn les_tailles_sont_lisibles() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(812), "812 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn les_durees_sont_lisibles() {
        assert_eq!(human_duration(Duration::from_secs(0)), "0 s");
        assert_eq!(human_duration(Duration::from_secs(59)), "59 s");
        assert_eq!(human_duration(Duration::from_secs(60)), "1 min 0 s");
        assert_eq!(human_duration(Duration::from_secs(130)), "2 min 10 s");
    }

    #[test]
    fn seuls_done_et_error_sont_regles() {
        assert!(!BatchStatus::Pending.is_settled());
        assert!(!BatchStatus::Running.is_settled());
        assert!(BatchStatus::Done.is_settled());
        assert!(BatchStatus::Failed.is_settled());
    }
}
