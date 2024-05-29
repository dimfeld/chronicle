use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use reqwest::{header::CONTENT_TYPE, Response};
use tracing::instrument;

use super::{ChatModelProvider, SendRequestOptions, SingleProviderResponse};
use crate::{
    format::{ChatRequestTransformation, SingleChatResponse},
    request::{parse_response_json, send_standard_request},
    Error,
};

/// OpenAI or fully-compatible provider
#[derive(Debug)]
pub struct OpenAi {
    client: reqwest::Client,
    token: Option<String>,
    url: String,
}

impl OpenAi {
    /// Create a new proxy for the OpenAI service
    pub fn new(client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            client,
            token: token.or_else(|| std::env::var("OPENAI_API_KEY").ok()),
            url: "https://api.openai.com/v1/chat/completions".into(),
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for OpenAi {
    fn name(&self) -> &str {
        "openai"
    }

    fn label(&self) -> &str {
        "OpenAI"
    }

    #[instrument(skip(self))]
    async fn send_request(
        &self,
        options: SendRequestOptions,
    ) -> Result<SingleProviderResponse, Report<Error>> {
        send_openai_request(
            &self.client,
            &self.url,
            None,
            self.token.as_deref(),
            &ChatRequestTransformation {
                supports_message_name: false,
                system_in_messages: true,
                strip_model_prefix: Some("openai/".into()),
            },
            options,
        )
        .await
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("openai/") || model.starts_with("gpt-")
    }
}

pub async fn send_openai_request(
    client: &reqwest::Client,
    url: &str,
    headers: Option<&reqwest::header::HeaderMap>,
    provider_token: Option<&str>,
    transform: &ChatRequestTransformation<'_>,
    SendRequestOptions {
        override_url,
        timeout,
        api_key,
        mut body,
    }: SendRequestOptions,
) -> Result<SingleProviderResponse, Report<Error>> {
    body.transform(transform);

    let bytes = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
    let bytes = Bytes::from(bytes);

    let token = api_key
        .as_deref()
        .or(provider_token)
        // Allow no API key since we could be sending to an internal OpenAI-compatible service.
        .unwrap_or_default();

    let (response, latency) = send_standard_request(
        timeout,
        || {
            client
                .post(override_url.as_deref().unwrap_or(url))
                .bearer_auth(token)
                .header(CONTENT_TYPE, "application/json; charset=utf8")
                .headers(headers.cloned().unwrap_or_default())
        },
        handle_rate_limit_headers,
        bytes,
    )
    .await
    .change_context(Error::ModelError)?;

    let result: SingleChatResponse = parse_response_json(response, latency)
        .await
        .change_context(Error::ModelError)?;

    Ok(SingleProviderResponse {
        model: result.model.clone().or(body.model).unwrap_or_default(),
        body: result,
        latency,
        meta: None,
    })
}

pub fn handle_rate_limit_headers(res: &Response) -> Option<Duration> {
    let headers = res.headers();
    let req_limit = headers
        .get("x-ratelimit-limit-requests")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_remaining = headers
        .get("x-ratelimit-remaining-requests")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let req_reset = headers
        .get("x-ratelimit-reset-requests")
        .and_then(|s| s.to_str().ok());
    let token_limit = headers
        .get("x-ratelimit-limit-tokens")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_remaining = headers
        .get("x-ratelimit-remaining-tokens")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let token_reset = headers
        .get("x-ratelimit-reset-tokens")
        .and_then(|s| s.to_str().ok());
    tracing::warn!(
        llm.rate_limiting.req_limit = req_limit,
        llm.rate_liting.req_remaining = req_remaining,
        llm.rate_limiting.req_reset = req_reset,
        llm.rate_limiting.token_limit = token_limit,
        llm.rate_liting.token_remaining = token_remaining,
        llm.rate_limiting.token_reset = token_reset,
        "Hit OpenAI rate limit"
    );

    None
    // TODO The reset times are Go-style durations. Need to parse that.

    /*
    let token_reset = token_remaining
        .zip(token_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| {
            chrono::DateTime::parse_from_rfc3339(reset_time).ok()
        });

    let req_reset = req_remaining
        .zip(req_reset)
        .filter(|(remaining, _)| *remaining == 0)
        .and_then(|(_, reset_time)| {
            chrono::DateTime::parse_from_rfc3339(reset_time).ok()
        });

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
    */
}
