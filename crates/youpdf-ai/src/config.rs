//! Configuration persistée de l'application.
//!
//! La configuration est un fichier JSON dans le répertoire utilisateur :
//!
//! - Linux : `~/.config/youpdf/settings.json`
//! - macOS : `~/Library/Application Support/youpdf/settings.json`
//! - Windows : `%APPDATA%\youpdf\settings.json`
//!
//! **Aucune clé API n'est stockée ici.** Seules les métadonnées des
//! fournisseurs (identifiant, type, URL, modèle par défaut) le sont.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result, Theme};

/// Type de fournisseur — détermine l'adaptateur à instancier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    OpenAiCompat,
    Anthropic,
    Gemini,
    Ollama,
}

/// Configuration persistée d'un fournisseur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Identifiant stable (sert de clé de stockage).
    pub id: String,
    /// Type, qui détermine l'adaptateur à utiliser.
    pub kind: ProviderKind,
    /// Nom affiché à l'utilisateur.
    pub display_name: String,
    /// URL de base personnalisée.
    pub base_url: Option<String>,
    /// Fournisseur activé ou non.
    pub enabled: bool,
    /// Modèle par défaut pour ce fournisseur.
    pub default_model: Option<String>,
}

impl ProviderConfig {
    /// Construit la configuration par défaut pour un type donné.
    pub fn for_kind(
        kind: ProviderKind,
        id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            display_name: display_name.into(),
            base_url: None,
            enabled: false,
            default_model: None,
        }
    }
}

/// Configuration globale de l'application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Interrupteur général de l'IA.
    ///
    /// Faux par défaut : tant qu'il est faux, YouPDF ne construit aucun
    /// fournisseur et n'ouvre aucune connexion réseau. C'est le seul moyen
    /// d'être certain qu'un document ne part nulle part.
    #[serde(default)]
    pub ai_enabled: bool,
    /// Thème visuel actif (index dans `Theme::ALL`).
    pub theme_index: usize,
    /// Langue d'interface (`fr`, `en`…).
    pub locale: String,
    /// Zoom par défaut à l'ouverture d'un document.
    pub default_zoom: f32,
    /// Fournisseurs d'IA configurés.
    pub providers: Vec<ProviderConfig>,
    /// Fournisseur actif.
    pub active_provider: Option<String>,
    /// Modèle actif.
    pub active_model: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_enabled: false,
            theme_index: 0,
            locale: "en".to_string(),
            default_zoom: 1.0,
            providers: vec![
                ProviderConfig::for_kind(ProviderKind::Ollama, "ollama", "Ollama (local)"),
                ProviderConfig::for_kind(ProviderKind::OpenAi, "openai", "OpenAI"),
                ProviderConfig::for_kind(ProviderKind::Anthropic, "anthropic", "Anthropic Claude"),
                ProviderConfig::for_kind(ProviderKind::OpenAiCompat, "openrouter", "OpenRouter"),
                ProviderConfig::for_kind(ProviderKind::Gemini, "gemini", "Google Gemini"),
            ],
            active_provider: None,
            active_model: None,
        }
    }
}

impl AppSettings {
    /// Répertoire de configuration de l'application.
    fn config_dir() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "youpdf", "youpdf").ok_or_else(|| {
            Error::Config("impossible de déterminer le répertoire de configuration".into())
        })?;
        Ok(dirs.config_dir().to_path_buf())
    }

    /// Chemin du fichier `settings.json`.
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("settings.json"))
    }

    /// Charge la configuration depuis le disque.
    ///
    /// Si le fichier n'existe pas, retourne les valeurs par défaut sans
    /// erreur (premier lancement).
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            tracing::info!("aucune configuration existante, valeurs par défaut");
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| Error::Config(format!("lecture de {} : {e}", path.display())))?;
        let settings: Self = serde_json::from_str(&raw)
            .map_err(|e| Error::Config(format!("parsing de {} : {e}", path.display())))?;
        Ok(settings)
    }

    /// Enregistre la configuration sur le disque.
    ///
    /// Crée le répertoire parent si nécessaire.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Config(format!("création de {} : {e}", parent.display())))?;
        }
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("sérialisation : {e}")))?;
        std::fs::write(&path, raw)
            .map_err(|e| Error::Config(format!("écriture de {} : {e}", path.display())))?;
        tracing::debug!(path = %path.display(), "configuration enregistrée");
        Ok(())
    }

    /// Thème actif (résolu depuis `theme_index`).
    pub fn theme(&self) -> Theme {
        Theme::ALL
            .get(self.theme_index)
            .copied()
            .unwrap_or(Theme::Dark)
    }
}
