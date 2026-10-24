//! Moteur Tesseract : détection, exécution capée, parsing TSV.
//!
//! Aucun de ces calculs ne touche au document : on lance un processus
//! externe sur un bitmap temporaire.

use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};


use youpdf_shared::{Error, Result};

use crate::render::PageBitmap;

use super::{OcrEngine, OcrMode, OcrResult, OcrWord, validate_language, which};

// Moteur Tesseract
// ────────────────────────────────────────────────────────────────────────

pub struct TesseractEngine {
    binary: Option<PathBuf>,
}
/// Lit un tuyau de bout en bout dans un thread dédié et renvoie le
/// récepteur. Générique car `stdout` et `stderr` sont deux types distincts.
pub(crate) fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

impl TesseractEngine {
    /// Garde-fou : une page de scan haute résolution peut faire tourner
    /// tesseract plusieurs dizaines de secondes. Sans délai, l'appelant
    /// (souvent le thread d'interface) reste bloqué indéfiniment.
    const TIMEOUT: Duration = Duration::from_secs(120);

    pub fn detect() -> Self {
        let binary = which("tesseract").ok();
        match &binary {
            Some(path) => tracing::info!(path = %path.display(), "tesseract détecté"),
            None => tracing::warn!("tesseract introuvable dans le PATH"),
        }
        Self { binary }
    }

    /// Lance un processus avec une **durée maximale**, en collectant stdout
    /// et stderr dans des threads séparés.
    ///
    /// La lecture séquentielle (`Command::output()`) deadlockerait si le
    /// second tuyau se remplit pendant qu'on lit le premier ; le délai ne
    /// serait alors jamais atteint. D'où les lecteurs dédiés.
    fn run_capped(cmd: &mut Command, timeout: Duration) -> Result<std::process::Output> {
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(Error::Io)?;

        let out_rx = spawn_reader(child.stdout.take());
        let err_rx = spawn_reader(child.stderr.take());

        let started = Instant::now();
        let status = loop {
            match child.try_wait().map_err(Error::Io)? {
                Some(status) => break status,
                None if started.elapsed() >= timeout => {
                    // On tue puis on récolte : sans `wait`, le processus
                    // resterait zombie pour toute la vie de l'application.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Internal(format!(
                        "tesseract interrompu après {} s : page trop volumineuse ou \
                         moteur bloqué",
                        timeout.as_secs()
                    )));
                }
                None => thread::sleep(Duration::from_millis(20)),
            }
        };

        let stdout = out_rx.recv().unwrap_or_default();
        let stderr = err_rx.recv().unwrap_or_default();
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    }

    /// Écrit un bitmap RGBA sous forme de fichier PPM (P6).
    fn write_ppm(&self, path: &std::path::Path, bitmap: &PageBitmap) -> Result<()> {
        let mut file = std::fs::File::create(path).map_err(Error::Io)?;
        write!(file, "P6\n{} {}\n255\n", bitmap.width, bitmap.height).map_err(Error::Io)?;

        let mut rgb = Vec::with_capacity(bitmap.width as usize * bitmap.height as usize * 3);
        for chunk in bitmap.pixels.chunks_exact(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }
        file.write_all(&rgb).map_err(Error::Io)?;
        Ok(())
    }

    /// Lance tesseract et retourne la sortie TSV parsée.
    fn recognize_tsv(
        &self,
        binary: &std::path::Path,
        ppm_path: &std::path::Path,
        language: &str,
    ) -> Result<(String, Vec<OcrWord>)> {
        let output = Self::run_capped(
            Command::new(binary)
                .arg(ppm_path)
                .arg("stdout")
                .arg("-l")
                .arg(language)
                .arg("tsv"),
            Self::TIMEOUT,
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Internal(format!(
                "tesseract a échoué : {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut words = Vec::new();
        let mut full_text = String::new();

        for (i, line) in stdout.lines().enumerate() {
            if i == 0 {
                continue; // en-tête
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 12 {
                continue;
            }
            // Colonnes TSV : level, page_num, block_num, par_num, line_num,
            // word_num, left, top, width, height, conf, text
            let level = fields[0].parse::<i32>().unwrap_or(0);
            if level != 5 {
                continue; // on ne garde que les mots
            }
            let parse = |s: &str| s.parse::<i32>().unwrap_or(0);
            let conf = parse(fields[10]);
            let text = fields[11].trim();
            if text.is_empty() || conf < 30 {
                continue;
            }
            words.push(OcrWord {
                left: parse(fields[6]),
                top: parse(fields[7]),
                width: parse(fields[8]),
                height: parse(fields[9]),
                confidence: conf,
                text: text.to_string(),
            });
            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(text);
        }

        Ok((full_text, words))
    }
}

impl OcrEngine for TesseractEngine {
    fn display_name(&self) -> &str {
        match self.binary {
            Some(_) => "Tesseract (système)",
            None => "Tesseract (non installé)",
        }
    }

    fn available_languages(&self) -> Result<Vec<String>> {
        let Some(binary) = &self.binary else {
            return Err(Error::NotSupported("tesseract non installé".into()));
        };
        let output =
            Self::run_capped(Command::new(binary).arg("--list-langs"), Self::TIMEOUT)?;
        if !output.status.success() {
            return Err(Error::Internal("échec de --list-langs".into()));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    fn recognize(
        &self,
        bitmap: &PageBitmap,
        page: usize,
        language: &str,
        mode: OcrMode,
    ) -> Result<OcrResult> {
        let Some(binary) = &self.binary else {
            return Err(Error::NotSupported(
                "OCR indisponible : tesseract n'est pas installé.\n\
                 • Debian/Ubuntu : sudo apt install tesseract-ocr tesseract-ocr-fra\n\
                 • macOS : brew install tesseract tesseract-lang\n\
                 • Windows : https://github.com/UB-Mannheim/tesseract/wiki"
                    .into(),
            ));
        };

        let tmp = tempfile::Builder::new()
            .prefix("youpdf-ocr-")
            .suffix(".ppm")
            .tempfile()
            .map_err(Error::Io)?;
        self.write_ppm(tmp.path(), bitmap)?;

        let language = validate_language(language)?;
        let (text, words) = self.recognize_tsv(binary, tmp.path(), language)?;

        tracing::info!(page, language, words = words.len(), "OCR terminé");

        Ok(OcrResult {
            page,
            char_count: text.chars().count(),
            text,
            language: language.to_string(),
            mode,
            words,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────
