//! Read-aloud / text-to-speech via macOS `say`.
//!
//! ponytail: Shell out to the platform binary rather than binding AVFoundation.
//! Windows/Linux stubs return Err("Platform not supported"); add
//! speech-dispatcher / SAPI / flameshot later if cross-platform is needed.

use std::process::{Child, Command};
use youpdf_shared::{Error, Result};

/// Moteur de synthèse vocale locale, un processus `say` à la fois.
pub struct TtsEngine {
    pub child: Option<Child>,
    pub current_voice: String,
    pub rate: f32, // words per minute
}

impl Default for TtsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsEngine {
    /// Crée un moteur par défaut : voix Alice, 175 mots/min.
    pub fn new() -> Self {
        Self {
            child: None,
            current_voice: "Alice".to_string(),
            rate: 175.0,
        }
    }

    /// Liste les voix disponibles en parsant `say -v ?`.
    ///
    /// Chaque ligne a le format `Nom  # Langage` (ex. `Alice  # English`).
    /// Sur une plateforme sans `say`, renvoie un vecteur vide.
    pub fn list_voices() -> Vec<String> {
        let Ok(output) = Command::new("say").arg("-v").arg("?").output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, '#').collect();
                parts
                    .first()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .collect()
    }

    /// Lance la lecture de `text` et arrête toute lecture précédente.
    pub fn speak(&mut self, text: &str) -> Result<()> {
        self.stop();
        let child = Command::new("say")
            .arg("-v")
            .arg(&self.current_voice)
            .arg("-r")
            .arg(self.rate.to_string())
            .arg(text)
            .spawn()
            .map_err(|e| Error::Pdf(format!("say spawn: {e}")))?;
        self.child = Some(child);
        Ok(())
    }

    /// Arrête la lecture en cours (s'il y en a une).
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }

    /// Vrai si un processus `say` est encore en cours d'exécution.
    ///
    /// `try_wait` a besoin de `&mut self` (le processus est récolté une seule fois),
    /// donc cette méthode consomme le verrou mutable ; les appelateurs doivent
    /// appeler `is_playing` sur une instance `mut`.
    pub fn is_playing(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => child.try_wait().map_or(true, |r| r.is_none()),
            None => false,
        }
    }

    /// Change la voix courante.
    pub fn set_voice(&mut self, voice: String) {
        self.current_voice = voice;
    }

    /// Change la vitesse de lecture (mots par minute), bornée entre 100 et 300.
    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.clamp(100.0, 300.0);
    }
}