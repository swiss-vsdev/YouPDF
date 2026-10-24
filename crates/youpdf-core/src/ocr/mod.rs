//! Reconnaissance optique de caractères (OCR) et réinjection.
//!
//! ## Deux usages
//!
//! 1. **Extraction de texte** : reconnaître le texte d'une page et le
//!    retourner sous forme de chaîne (utile pour le chat IA, la
//!    recherche, le copier-coller).
//! 2. **PDF consultable** : transformer un scan en PDF où le texte est
//!    sélectionnable et cherchable, sans changer l'apparence visuelle.
//!    C'est le mode « PDF consultable » du standard de l'industrie.
//!
//! ## Chaîne de traitement
//!
//! ```text
//! PDF → pdfium render → PageBitmap (RGBA)
//!                     → PPM temporaire
//!                     → tesseract → texte + positions
//!                     → injection dans le PDF
//! ```

use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result};

use crate::render::PageBitmap;

/// Mode de sortie de l'OCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OcrMode {
    /// PDF modifiable : texte et images reconnus, fichier léger.
    Editable,
    /// Texte + images : image de fond conservée, texte superposé.
    TextAndImage,
    /// PDF consultable : image conservée + calque texte invisible.
    #[default]
    Searchable,
}

/// Une langue reconnue par tesseract.
///
/// Le catalogue est figé dans le binaire : il dit à l'utilisateur ce que
/// tesseract **sait** faire, même quand le paquet de langue n'est pas
/// encore installé sur la machine. La liste réellement disponible est
/// obtenue par [`OcrEngine::available_languages`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrLanguage {
    /// Code tesseract (`fra`, `chi_sim`, `old_deu`…).
    pub code: &'static str,
    /// Nom de la langue, en anglais.
    pub name: &'static str,
}

/// Les langues usuuelles, dans l'ordre alphabétique anglais.
///
/// Le catalogue est plus large que la sélection par défaut : tesseract
/// s'installe avec des paquets de langue séparés, et un utilisateur doit
/// pouvoir **nommer** la langue qu'il veut même si elle n'est pas encore
/// installée.
pub const LANGUAGES: &[OcrLanguage] = &[
    OcrLanguage { code: "afr", name: "Afrikaans" },
    OcrLanguage { code: "ara", name: "Arabic" },
    OcrLanguage { code: "aze", name: "Azerbaijani" },
    OcrLanguage { code: "bel", name: "Belarusian" },
    OcrLanguage { code: "ben", name: "Bengali" },
    OcrLanguage { code: "bul", name: "Bulgarian" },
    OcrLanguage { code: "cat", name: "Catalan" },
    OcrLanguage { code: "ces", name: "Czech" },
    OcrLanguage { code: "chi_sim", name: "Chinese (Simplified)" },
    OcrLanguage { code: "chi_tra", name: "Chinese (Traditional)" },
    OcrLanguage { code: "dan", name: "Danish" },
    OcrLanguage { code: "deu", name: "German" },
    OcrLanguage { code: "ell", name: "Greek" },
    OcrLanguage { code: "eng", name: "English" },
    OcrLanguage { code: "est", name: "Estonian" },
    OcrLanguage { code: "eus", name: "Basque" },
    OcrLanguage { code: "fas", name: "Persian" },
    OcrLanguage { code: "fin", name: "Finnish" },
    OcrLanguage { code: "fra", name: "French" },
    OcrLanguage { code: "heb", name: "Hebrew" },
    OcrLanguage { code: "hin", name: "Hindi" },
    OcrLanguage { code: "hrv", name: "Croatian" },
    OcrLanguage { code: "hun", name: "Hungarian" },
    OcrLanguage { code: "ind", name: "Indonesian" },
    OcrLanguage { code: "isl", name: "Icelandic" },
    OcrLanguage { code: "ita", name: "Italian" },
    OcrLanguage { code: "jpn", name: "Japanese" },
    OcrLanguage { code: "kor", name: "Korean" },
    OcrLanguage { code: "lat", name: "Latin" },
    OcrLanguage { code: "lav", name: "Latvian" },
    OcrLanguage { code: "lit", name: "Lithuanian" },
    OcrLanguage { code: "mkd", name: "Macedonian" },
    OcrLanguage { code: "msa", name: "Malay" },
    OcrLanguage { code: "nld", name: "Dutch" },
    OcrLanguage { code: "nor", name: "Norwegian" },
    OcrLanguage { code: "pol", name: "Polish" },
    OcrLanguage { code: "por", name: "Portuguese" },
    OcrLanguage { code: "ron", name: "Romanian" },
    OcrLanguage { code: "rus", name: "Russian" },
    OcrLanguage { code: "slk", name: "Slovak" },
    OcrLanguage { code: "slv", name: "Slovenian" },
    OcrLanguage { code: "spa", name: "Spanish" },
    OcrLanguage { code: "srp", name: "Serbian" },
    OcrLanguage { code: "swe", name: "Swedish" },
    OcrLanguage { code: "tha", name: "Thai" },
    OcrLanguage { code: "tur", name: "Turkish" },
    OcrLanguage { code: "ukr", name: "Ukrainian" },
    OcrLanguage { code: "vie", name: "Vietnamese" },
];

/// Le nom lisible d'un code de langue.
///
/// Un code inconnu est renvoyé tel quel : l'utilisateur peut saisir une
/// langue que tesseract sait mais que le catalogue ne connaît pas.
pub fn language_name(code: &str) -> String {
    match LANGUAGES.iter().find(|lang| lang.code == code) {
        Some(lang) => lang.name.to_string(),
        None => code.to_string(),
    }
}

/// Assemble plusieurs langues en un identifiant tesseract.
///
/// Tesseract sépare les langues par `+` : `eng+fra` reconnaît un document
/// qui mélange les deux. L'ordre est conservé, les doublons sont écartés.
pub fn combine_languages(codes: &[String]) -> Result<String> {
    let mut parts: Vec<&str> = Vec::new();
    for code in codes {
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        if !parts.contains(&code) {
            parts.push(code);
        }
    }
    if parts.is_empty() {
        return Err(invalid_language(""));
    }
    let combined = parts.join("+");
    validate_language(&combined)?;
    Ok(combined)
}


impl OcrMode {
    pub const ALL: &'static [(OcrMode, &'static str, &'static str)] = &[
        (
            OcrMode::Editable,
            "PDF modifiable",
            "Texte et images reconnus, fichier léger",
        ),
        (
            OcrMode::TextAndImage,
            "Texte + images",
            "Image de fond conservée, texte superposé",
        ),
        (
            OcrMode::Searchable,
            "PDF consultable",
            "Apparence inchangée, texte sélectionnable",
        ),
    ];
}

/// Résultat d'une reconnaissance OCR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub page: usize,
    pub text: String,
    pub language: String,
    pub mode: OcrMode,
    pub char_count: usize,
    /// Mots avec leurs boîtes englobantes (coordonnées écran bitmap).
    #[serde(skip)]
    pub words: Vec<OcrWord>,
}

/// Un mot reconnu avec sa position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrWord {
    /// Coordonnées dans le bitmap (origine haut-gauche).
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    /// Confiance (0–100).
    pub confidence: i32,
    /// Texte.
    pub text: String,
}

/// Interface d'un moteur OCR.
pub trait OcrEngine: Send + Sync {
    fn recognize(
        &self,
        bitmap: &PageBitmap,
        page: usize,
        language: &str,
        mode: OcrMode,
    ) -> Result<OcrResult>;

    fn available_languages(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn display_name(&self) -> &str;
}

// ────────────────────────────────────────────────────────────────────────

pub mod tesseract;
pub mod searchable;

/// Valide un identifiant de langue avant de le transmettre à tesseract.
///
/// `Command::arg` ne passe **pas** par un shell : il n'y a donc pas
/// d'injection de commandes (`; rm -rf /` arriverait comme un simple
/// argument). Le risque réel est ailleurs : tesseract accepte des options
/// dans `-l` et lit des fichiers de configuration (`--configfile`,
/// `tessedit_*`), si bien qu'une valeur arbitraire peut modifier son
/// comportement ou lui faire lire un chemin choisi par un tiers — or un OCR
/// est souvent lancé sur un PDF reçu de l'extérieur.
pub fn validate_language(language: &str) -> Result<&str> {
    if language.is_empty() || language.len() > 64 {
        return Err(invalid_language(language));
    }
    // Chaque segment séparé par `+` est soit un code (2–3 lettres, ou un
    // nom long comme `old_deu`), soit la forme `script/Nom` de tesseract.
    let ok = language.split('+').all(|part| match part.split_once('/') {
        Some((prefix, name)) => {
            prefix == "script"
                && !name.is_empty()
                && name.len() <= 16
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '_')
        }
        None => {
            part.len() >= 2
                && part.len() <= 16
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphabetic() || c == '_')
        }
    });
    if ok {
        Ok(language)
    } else {
        Err(invalid_language(language))
    }
}

pub(crate) fn invalid_language(language: &str) -> youpdf_shared::Error {
    Error::InvalidInput(format!(
        "langage OCR invalide : {language:?} (attendu un code type `fra` ou \
         `eng+fra` ; la liste est disponible via `available_languages`)"
    ))
}

// ────────────────────────────────────────────────────────────────────────
// Helper `which`
// ────────────────────────────────────────────────────────────────────────

pub(crate) fn which(name: &str) -> std::result::Result<PathBuf, ()> {
    let path_var = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join(format!("{name}.exe"));
            if is_executable(&with_exe) {
                return Ok(with_exe);
            }
        }
    }
    Err(())
}

pub(crate) fn is_executable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        true
    }
}

pub use tesseract::{TesseractEngine};
pub use searchable::{make_ocr, make_searchable};
