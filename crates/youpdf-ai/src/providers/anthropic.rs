//! Adaptateur pour l'API Anthropic Claude.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result};

use crate::message::{ChatOptions, ChatResponse, Message, ModelInfo, Role, Usage};
use crate::provider::AiProvider;
use crate::registry::ProviderId;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    id: ProviderId,
    display_name: String,
    client: Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(id: ProviderId, display_name: String, api_key: String) -> Result<Self> {
        let base_url = DEFAULT_BASE_URL.to_string();
        let client = crate::endpoint::http_client(&base_url, true)?;
        Ok(Self {
            id,
            display_name,
            client,
            base_url,
            api_key,
        })
    }
}

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[derive(Debug, Serialize)]
struct WireMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    model: String,
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![
            model("claude-3-5-sonnet-20241022", 200_000),
            model("claude-3-5-haiku-20241022", 200_000),
            model("claude-3-opus-20240229", 200_000),
        ])
    }

    async fn chat(
        &self,
        model_name: &str,
        messages: &[Message],
        options: &ChatOptions,
    ) -> Result<ChatResponse> {
        let system = messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.as_str());

        let conversational: Vec<WireMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| WireMessage {
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => unreachable!("filtré ci-dessus"),
                },
                content: &m.content,
            })
            .collect();

        let body = MessagesRequest {
            model: model_name,
            max_tokens: options.max_tokens.unwrap_or(4096),
            system,
            messages: conversational,
            temperature: options.temperature,
            top_p: options.top_p,
        };

        let url = format!("{}/messages", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("{url} : {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Ai(format!("{url} → HTTP {status} : {body}")));
        }

        let parsed: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| Error::Ai(format!("réponse /messages invalide : {e}")))?;

        let content = parsed
            .content
            .into_iter()
            .filter(|b| b.kind == "text")
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ChatResponse {
            content,
            model: parsed.model,
            usage: parsed.usage.map(|u| Usage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            }),
        })
    }
}

fn model(id: &str, ctx: u32) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        display_name: id.to_string(),
        context_window: Some(ctx),
    }
}
