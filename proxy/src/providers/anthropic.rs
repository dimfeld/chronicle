use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use itertools::Itertools;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use super::{ChatModelProvider, ProviderResponse, SendRequestOptions};
use crate::{
    format::{
        ChatChoice, ChatMessage, ChatRequest, ChatRequestTransformation, ChatResponse,
        UsageResponse,
    },
    request::{send_standard_request, RetryOptions},
    Error,
};

#[derive(Debug)]
pub struct Anthropic {
    client: reqwest::Client,
    token: Option<String>,
}

impl Anthropic {
    pub fn new(client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            client,
            token: token.or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Anthropic {
    fn name(&self) -> &str {
        "Anthropic"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            retry_options,
            timeout,
            api_key,
            mut body,
        }: SendRequestOptions,
    ) -> Result<ProviderResponse, Report<Error>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: false,
            strip_model_prefix: Some("anthropic/".into()),
        });

        // We could do something here to simulate the `n` parameter but don't right now.
        // If we do then this should be done in a layer outside the provider.
        body.n = None;
        // Clear out some fields that Anthropic doesn't use.
        body.frequency_penalty = None;
        body.logit_bias = None;
        body.logprobs = None;
        body.presence_penalty = None;
        body.response_format = None;
        body.seed = None;
        body.top_logprobs = None;
        body.user = None;

        let body = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let body = Bytes::from(body);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or(Error::MissingApiKey)?;

        let result = send_standard_request::<AnthropicChatResponse>(
            retry_options,
            timeout,
            || {
                self.client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", api_token)
                    .header("anthropic-version", "2023-06-01")
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            handle_retry_after,
            body,
        )
        .await?;

        Ok(ProviderResponse {
            body: result.data.0.into(),
            meta: None,
            retries: result.num_retries,
            rate_limited: result.was_rate_limited,
            latency: result.data.1,
        })
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("anthropic/") || model.starts_with("claude")
    }
}

fn handle_retry_after(res: &reqwest::Response) -> Option<Duration> {
    let headers = res.headers();
    let req_limit = headers
        .get("anthropic-ratelimit-requests-limit")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_remaining = headers
        .get("anthropic-ratelimit-requests-remaining")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_reset = headers
        .get("anthropic-ratelimit-requests-reset")
        .and_then(|s| s.to_str().ok());
    let token_limit = headers
        .get("anthropic-ratelimit-tokens-limit")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_remaining = headers
        .get("anthropic-ratelimit-tokens-remaining")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_reset = headers
        .get("anthropic-ratelimit-tokens-reset")
        .and_then(|s| s.to_str().ok());
    tracing::warn!(
        req_limit,
        req_remaining,
        req_reset,
        token_limit,
        token_remaining,
        token_reset,
        "Hit Anthropic rate limit"
    );

    let token_reset = token_remaining
        .zip(token_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| chrono::DateTime::parse_from_rfc3339(reset_time).ok());

    let req_reset = req_remaining
        .zip(req_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| chrono::DateTime::parse_from_rfc3339(reset_time).ok());

    let reset_time = match (token_reset, req_reset) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let until_reset_time = reset_time
        .map(|time| time.to_utc() - chrono::Utc::now())
        .and_then(|d| d.to_std().ok());

    until_reset_time
}

/// A chat response, in non-chunked format
#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicChatResponse {
    // Omitted certain fields that aren't really useful
    // id: String,
    // type: String,
    pub role: String,
    pub content: Vec<AnthropicChatContent>,
    pub model: String,
    pub stop_reason: String,
    pub stop_sequence: String,
    pub usage: AnthropicUsageResponse,
}

impl Into<ChatResponse> for AnthropicChatResponse {
    fn into(mut self) -> ChatResponse {
        let text = if self.content.len() == 1 {
            match self.content.pop().unwrap() {
                AnthropicChatContent::Text { text } => text,
            }
        } else {
            self.content
                .iter()
                .map(|c| match c {
                    AnthropicChatContent::Text { text } => text,
                })
                .join("")
        };

        ChatResponse {
            created: chrono::Utc::now().timestamp() as u64,
            model: Some(self.model),
            system_fingerprint: None,
            choices: vec![ChatChoice {
                index: 0,
                // TODO align this with OpenAI finish_reason
                finish_reason: self.stop_reason,
                message: ChatMessage {
                    role: self.role,
                    name: None,
                    content: text,
                },
            }],
            usage: UsageResponse {
                prompt_tokens: Some(self.usage.input_tokens),
                completion_tokens: Some(self.usage.output_tokens),
                total_tokens: Some(self.usage.input_tokens + self.usage.output_tokens),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicChatContent {
    Text { text: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicUsageResponse {
    pub input_tokens: usize,
    pub output_tokens: usize,
}
