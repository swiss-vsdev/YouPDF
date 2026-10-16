//! Le trait unique [`AiProvider`].

use async_trait::async_trait;

use youpdf_shared::Result;

use crate::message::{ChatOptions, ChatResponse, Message, ModelInfo};
use crate::registry::ProviderId;

/// Le contrat que doit respecter tout fournisseur d'IA.
#[async_trait]
pub trait AiProvider: Send + Sync + 'static {
    /// Identifiant stable et unique du fournisseur.
    fn id(&self) -> &ProviderId;

    /// Nom lisible affiché à l'utilisateur.
    fn display_name(&self) -> &str;

    /// Liste les modèles disponibles chez ce fournisseur.
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Envoie une conversation et renvoie la réponse du modèle.
    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse>;

    /// Indique si ce fournisseur supporte les embeddings.
    fn supports_embeddings(&self) -> bool {
        false
    }

    /// Calcule les embeddings d'une liste de textes.
    async fn embed(&self, _model: &str, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Err(youpdf_shared::Error::NotSupported(
            "embeddings non pris en charge par ce fournisseur".into(),
        ))
    }
}
