use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use tracing::instrument;

use super::{ChatModelProvider, ProviderResponse};
use crate::{
    format::{ChatRequest, ChatRequestTransformation},
    request::{send_standard_request, RetryOptions},
    Error, ProxyRequestOptions,
};

/// OpenAI or fully-compatible provider
#[derive(Debug)]
pub struct OpenAi {
    name: String,
    client: reqwest::Client,
    // token prepended with "Token" for use in Bearer auth
    token: String,
    url: String,
}

#[async_trait::async_trait]
impl ChatModelProvider for OpenAi {
    fn name(&self) -> &str {
        &self.name
    }

    #[instrument(skip(self))]
    async fn send_request(
        &self,
        retry_options: RetryOptions,
        timeout: Duration,
        mut body: ChatRequest,
    ) -> Result<ProviderResponse, Report<Error>> {
        body.transform(ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: true,
            strip_model_prefix: Some("openai/"),
        });

        let body = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let body = Bytes::from(body);

        let result = send_standard_request(
            retry_options,
            timeout,
            || self.client.post(&self.url).bearer_auth(&self.token),
            |res| {
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
                    req_limit,
                    req_remaining,
                    req_reset,
                    token_limit,
                    token_remaining,
                    token_reset,
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
            },
            body,
        )
        .await?;

        Ok(ProviderResponse {
            body: result.data.0,
            meta: None,
            retries: result.num_retries,
            rate_limited: result.was_rate_limited,
            latency: result.data.1,
        })
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("openai/") || model.starts_with("gpt-")
    }
}
