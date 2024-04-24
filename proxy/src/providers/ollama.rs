use std::borrow::Cow;

use bytes::Bytes;
use chrono::Utc;
use error_stack::{Report, ResultExt};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{ChatModelProvider, ProviderResponse, SendRequestOptions};
use crate::{
    format::{ChatChoice, ChatMessage, ChatRequestTransformation, ChatResponse, UsageResponse},
    request::send_standard_request,
    Error,
};

#[derive(Debug)]
pub struct Ollama {
    pub url: String,
    client: reqwest::Client,
}

impl Ollama {
    pub fn new(client: reqwest::Client, url: Option<String>) -> Self {
        let url = url.as_deref().unwrap_or("http://localhost:11434");
        let url = format!("{url}/api/chat");

        Self { url, client }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Ollama {
    fn name(&self) -> &str {
        "ollama"
    }

    fn label(&self) -> &str {
        "Ollama"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            timeout,
            api_key,
            mut body,
        }: SendRequestOptions,
    ) -> Result<ProviderResponse, Report<Error>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: true,
            strip_model_prefix: Some(Cow::Borrowed("ollama/")),
        });

        let model = body.model.ok_or(Error::ModelNotSpecified)?;

        let request = OllamaChatRequest {
            model,
            messages: body.messages,
            options: OllamaModelOptions {
                temperature: body.temperature,
                top_p: body.top_p,
                stop: body.stop,
                num_predict: body.max_tokens,
                frequency_penalty: body.frequency_penalty,
                presence_penalty: body.presence_penalty,
                seed: body.seed,
            },
            stream: false,
            keep_alive: None,
        };

        let body = serde_json::to_vec(&request).change_context(Error::TransformingRequest)?;
        let body = Bytes::from(body);

        let now = Utc::now().timestamp();
        let result = send_standard_request::<OllamaResponse>(
            timeout,
            || {
                self.client
                    .post(&self.url)
                    .timeout(timeout)
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            // Ollama never returns a 429
            |_| None,
            body,
        )
        .await
        .change_context(Error::ModelError)?;

        let meta = json!({
            "load_duration": result.0.load_duration,
            "prompt_eval_duration": result.0.prompt_eval_duration,
            "eval_duration": result.0.eval_duration,
        });

        let response = ChatResponse {
            created: now as u64,
            model: Some(result.0.model),
            system_fingerprint: None,
            choices: vec![ChatChoice {
                index: 0,
                finish_reason: "stop".to_string(),
                message: result.0.message,
            }],
            usage: UsageResponse {
                prompt_tokens: Some(result.0.prompt_eval_count as usize),
                completion_tokens: Some(result.0.eval_count as usize),
                total_tokens: None,
            },
        };

        Ok(ProviderResponse {
            body: response,
            meta: Some(meta),
            latency: result.1,
        })
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("ollama/")
    }
}

#[derive(Serialize, Debug)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    options: OllamaModelOptions,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<String>,
}

#[derive(Serialize, Debug)]
struct OllamaModelOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    num_predict: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct OllamaResponse {
    created_at: String,
    model: String,
    message: ChatMessage,
    total_duration: u64,
    load_duration: u64,
    prompt_eval_count: u64,
    prompt_eval_duration: u64,
    eval_count: u64,
    eval_duration: u64,
}
