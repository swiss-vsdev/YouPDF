//! Adaptateurs pour les différents fournisseurs d'IA.

pub mod anthropic;
pub mod ollama;
pub mod openai_compat;

use std::sync::Arc;

use youpdf_shared::{Error, Result};

use crate::config::{ProviderConfig, ProviderKind};
use crate::keyring::SecretStore;
use crate::provider::AiProvider;
use crate::registry::ProviderId;

/// Construit un fournisseur à partir de sa configuration.
///
/// La clé API est récupérée automatiquement depuis le trousseau système
/// si elle n'est pas fournie explicitement.
pub fn build_provider(config: &ProviderConfig) -> Result<Arc<dyn AiProvider>> {
    let id = ProviderId::new(config.id.clone());

    match config.kind {
        ProviderKind::OpenAi => {
            let key = SecretStore::get_api_key(&config.id)?.ok_or_else(|| {
                Error::Config(format!(
                    "clé API manquante pour {}. Ajoutez-la dans les préférences.",
                    config.display_name
                ))
            })?;
            Ok(Arc::new(openai_compat::OpenAiCompatProvider::openai(
                id,
                config.display_name.clone(),
                key,
            )?))
        }

        ProviderKind::OpenAiCompat => {
            let base_url = config.base_url.clone().ok_or_else(|| {
                Error::Config(format!(
                    "URL de base requise pour '{}'",
                    config.display_name
                ))
            })?;
            let key = SecretStore::get_api_key(&config.id)?;
            Ok(Arc::new(openai_compat::OpenAiCompatProvider::custom(
                id,
                config.display_name.clone(),
                base_url,
                key,
            )?))
        }

        ProviderKind::Anthropic => {
            let key = SecretStore::get_api_key(&config.id)?.ok_or_else(|| {
                Error::Config(format!("clé API manquante pour {}", config.display_name))
            })?;
            Ok(Arc::new(anthropic::AnthropicProvider::new(
                id,
                config.display_name.clone(),
                key,
            )?))
        }

        ProviderKind::Ollama => {
            let base_url = config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            Ok(Arc::new(ollama::OllamaProvider::new(
                id,
                config.display_name.clone(),
                base_url,
            )?))
        }

        // Gemini expose un endpoint compatible OpenAI.
        ProviderKind::Gemini => {
            let key = SecretStore::get_api_key(&config.id)?.ok_or_else(|| {
                Error::Config(format!("clé API manquante pour {}", config.display_name))
            })?;
            let base_url = config.base_url.clone().unwrap_or_else(|| {
                "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
            });
            Ok(Arc::new(openai_compat::OpenAiCompatProvider::custom(
                id,
                config.display_name.clone(),
                base_url,
                Some(key),
            )?))
        }
    }
}
