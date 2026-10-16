//! Adaptateur pour toute API compatible OpenAI Chat Completions.
//!
//! Ce seul fichier couvre : OpenAI, Azure OpenAI, OpenRouter, Groq,
//! Together, Fireworks, LM Studio, vLLM, llama.cpp server, et tout
//! endpoint custom respectant le format.

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use serde::Deserialize;

use youpdf_shared::{Error, Result};

use crate::endpoint;
use crate::message::{ChatOptions, ChatResponse, Message, ModelInfo, Role, Usage};
use crate::provider::AiProvider;
use crate::registry::ProviderId;

pub struct OpenAiCompatProvider {
    id: ProviderId,
    display_name: String,
    client: Client,
    base_url: String,
    api_key: Option<String>,
    extra_headers: HeaderMap,
}

impl OpenAiCompatProvider {
    pub fn openai(id: ProviderId, display_name: String, api_key: String) -> Result<Self> {
        Self::new(
            id,
            display_name,
            "https://api.openai.com/v1".to_string(),
            Some(api_key),
        )
    }

    pub fn custom(
        id: ProviderId,
        display_name: String,
        base_url: String,
        api_key: Option<String>,
    ) -> Result<Self> {
        Self::new(id, display_name, base_url, api_key)
    }

    pub fn new(
        id: ProviderId,
        display_name: String,
        base_url: String,
        api_key: Option<String>,
    ) -> Result<Self> {
        // URL validée **et** normalisée : c'est le dernier point de contrôle
        // avant qu'une clé API ne parte vers cette adresse. `base_url` vient
        // d'un fichier de configuration que l'utilisateur n'a pas forcément
        // écrit à la main (import, script d'entreprise, dépôt partagé).
        let base_url = endpoint::parse_base_url(&base_url)?;
        let client = endpoint::http_client(&base_url, api_key.is_some())?;
        Ok(Self {
            id,
            display_name,
            client,
            base_url,
            api_key,
            extra_headers: HeaderMap::new(),
        })
    }

    pub fn with_header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let (Ok(n), Ok(v)) = (
            HeaderName::try_from(name.as_ref()),
            HeaderValue::try_from(value.as_ref()),
        ) {
            self.extra_headers.insert(n, v);
        }
        self
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        for (name, value) in &self.extra_headers {
            req = req.header(name, value);
        }
        req
    }
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Debug, serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

impl<'a> From<&'a Message> for WireMessage<'a> {
    fn from(m: &'a Message) -> Self {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        Self {
            role,
            content: &m.content,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatWireResponse {
    model: String,
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: WireResponseMessage,
}

#[derive(Debug, Deserialize)]
struct WireResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

#[async_trait]
impl AiProvider for OpenAiCompatProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.base_url);
        let resp = self
            .request(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /models invalide : {e}")))?;

        Ok(parsed
            .data
            .into_iter()
            .map(|m| ModelInfo {
                display_name: m.id.clone(),
                id: m.id,
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
        let url = format!("{}/chat/completions", self.base_url);
        let body = ChatRequest {
            model,
            messages: messages.iter().map(WireMessage::from).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            stop: options.stop.clone(),
        };

        let resp = self
            .request(reqwest::Method::POST, &url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: ChatWireResponse = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /chat/completions invalide : {e}")))?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: parsed.model,
            usage: parsed.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
        })
    }
}
