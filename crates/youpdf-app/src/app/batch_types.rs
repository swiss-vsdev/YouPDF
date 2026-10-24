//! Types de la fenêtre « Traitement par lots » (file, états, job).

use std::path::PathBuf;

use egui::Color32;

use youpdf_core::Document;

use super::*;

/// Un fichier de la file, avec ce que le lot en a fait.
pub(super) struct BatchEntry {
    pub(super) path: PathBuf,
    /// Nombre de pages, lu une fois à l'ajout.
    pub(super) pages: Option<usize>,
    /// Taille du fichier en octets.
    pub(super) size: u64,
    pub(super) status: BatchStatus,
    /// Message d'échec, affiché au survol.
    pub(super) detail: Option<String>,
}

impl BatchEntry {
    pub(super) fn new(path: PathBuf) -> Self {
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

    pub(super) fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub(super) fn reset(&mut self) {
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
    pub(super) fn is_settled(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }

    pub(super) fn label(self) -> (&'static str, Color32) {
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
pub(super) fn human_bytes(bytes: u64) -> String {
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
pub(super) fn human_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min {} s", seconds / 60, seconds % 60)
    }
}

/// Lit « 1-3, 7 » en numéros de page bornés au document.
pub(super) fn parse_page_numbers(text: &str, total: usize) -> Vec<usize> {
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
pub(super) struct BatchJob {
    pub(super) files: Vec<PathBuf>,
    pub(super) dir: PathBuf,
    pub(super) text: bool,
    pub(super) images: bool,
    pub(super) compress: bool,
    pub(super) protect: bool,
    pub(super) flatten: bool,
    pub(super) protect_options: EncryptionOptions,
    pub(super) convert: Option<OfficeFormat>,
    pub(super) remove_pages: String,
    pub(super) watermark_text: String,
    pub(super) header_text: String,
    pub(super) bates_prefix: String,
    pub(super) bates_start: u32,
    pub(super) combine: bool,
    pub(super) prefix: String,
    pub(super) suffix: String,
    pub(super) print: bool,
    pub(super) insert: Option<PathBuf>,
    pub(super) insert_at: usize,
    pub(super) ocr: bool,
    pub(super) ocr_language: String,
    pub(super) background: Option<Background>,
}

impl BatchJob {
    /// Traite chaque fichier et envoie une ligne par fichier, puis un bilan.
    ///
    /// Tourne dans un fil de travail : `Document` n'est pas `Send`, il est donc
    /// créé et détruit ici, et seules des `String` repartent vers l'interface.
    pub(super) fn run(
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
    pub(super) fn les_tailles_sont_lisibles() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(812), "812 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    pub(super) fn les_durees_sont_lisibles() {
        assert_eq!(human_duration(Duration::from_secs(0)), "0 s");
        assert_eq!(human_duration(Duration::from_secs(59)), "59 s");
        assert_eq!(human_duration(Duration::from_secs(60)), "1 min 0 s");
        assert_eq!(human_duration(Duration::from_secs(130)), "2 min 10 s");
    }

    #[test]
    pub(super) fn seuls_done_et_error_sont_regles() {
        assert!(!BatchStatus::Pending.is_settled());
        assert!(!BatchStatus::Running.is_settled());
        assert!(BatchStatus::Done.is_settled());
        assert!(BatchStatus::Failed.is_settled());
    }
}
