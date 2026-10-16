//! Adaptateur pour Ollama (inférence locale).

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result};

use crate::message::{ChatOptions, ChatResponse, Message, ModelInfo, Role};
use crate::provider::AiProvider;
use crate::registry::ProviderId;

pub struct OllamaProvider {
    id: ProviderId,
    display_name: String,
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(id: ProviderId, display_name: String, base_url: String) -> Result<Self> {
        // Ollama n'envoie aucun secret : HTTP local est légitime, et
        // l'URL est quand même validée (schéma, hôte, absence de
        // chemin d'injection).
        let base_url = crate::endpoint::parse_public_base_url(&base_url)?;
        let client = crate::endpoint::http_client(&base_url, false)?;
        Ok(Self {
            id,
            display_name,
            client,
            base_url,
        })
    }
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagEntry>,
}

#[derive(Debug, Deserialize)]
struct TagEntry {
    name: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<WireOptions>,
}

#[derive(Debug, Serialize)]
struct WireOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponseWire {
    model: String,
    message: WireResponseMessage,
}

#[derive(Debug, Deserialize)]
struct WireResponseMessage {
    content: String,
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn supports_embeddings(&self) -> bool {
        true
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: TagsResponse = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /api/tags invalide : {e}")))?;

        Ok(parsed
            .models
            .into_iter()
            .map(|m| ModelInfo {
                display_name: m.name.clone(),
                id: m.name,
                context_window: None,
            })
            .collect())
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let wire_messages: Vec<WireMessage> = messages
            .iter()
            .map(|m| WireMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: &m.content,
            })
            .collect();

        let has_options = options.temperature.is_some()
            || options.max_tokens.is_some()
            || options.top_p.is_some();

        let body = ChatRequest {
            model,
            messages: wire_messages,
            stream: false,
            // WireOptions est trivial : construction eager acceptée.
            options: has_options.then_some(WireOptions {
                temperature: options.temperature,
                num_predict: options.max_tokens,
                top_p: options.top_p,
            }),
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: ChatResponseWire = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /api/chat invalide : {e}")))?;

        Ok(ChatResponse {
            content: parsed.message.content,
            model: parsed.model,
            usage: None,
        })
    }

    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        #[derive(Serialize)]
        struct EmbedRequest<'a> {
            model: &'a str,
            input: &'a [String],
        }
        #[derive(Deserialize)]
        struct EmbedResponse {
            embeddings: Vec<Vec<f32>>,
        }

        let url = format!("{}/api/embed", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&EmbedRequest {
                model,
                input: texts,
            })
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /api/embed invalide : {e}")))?;

        Ok(parsed.embeddings)
    }
}
