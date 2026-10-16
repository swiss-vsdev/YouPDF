//! # youpdf-ai
//!
//! Couche IA **agnostique du fournisseur**.
//!
//! ## Principe fondateur
//!
//! YouPDF n'héberge aucune IA et n'impose aucun fournisseur. Toute la
//! diversité des API est encapsulée derrière **un seul trait** :
//! [`AiProvider`].
//!
//! ## Modules
//!
//! - [`config`]   : configuration persistée des fournisseurs
//! - [`endpoint`] : validation des URL de base et clients HTTP durcis
//! - [`keyring`]  : accès au trousseau système
//! - [`message`]  : types de message neutres
//! - [`provider`] : trait commun à tous les fournisseurs
//! - [`providers`] : adaptateurs (OpenAI, Anthropic, Ollama, compatibles)
//! - [`registry`] : registre des fournisseurs actifs
//! - [`tasks`]    : tâches IA de haut niveau (résumer, traduire, chat)

pub mod config;
pub mod endpoint;
pub mod keyring;
pub mod message;
pub mod provider;
pub mod providers;
pub mod registry;
pub mod tasks;

pub use config::{AppSettings, ProviderConfig, ProviderKind};
pub use endpoint::{http_client, parse_base_url, parse_public_base_url};
pub use keyring::SecretStore;
pub use message::{ChatOptions, ChatResponse, Message, ModelInfo, Role, Usage};
pub use provider::AiProvider;
pub use providers::build_provider;
pub use registry::{ProviderId, ProviderRegistry};
pub use tasks::{AiTask, ChatContext};
