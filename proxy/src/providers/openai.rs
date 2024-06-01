use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use eventsource_stream::Event;
use http::header::ACCEPT;
use reqwest::{header::CONTENT_TYPE, Response};
use tracing::instrument;

use super::{ChatModelProvider, ProviderError, ProviderErrorKind, SendRequestOptions};
use crate::{
    format::{
        ChatRequestTransformation, ResponseInfo, SingleChatResponse, StreamOptions,
        StreamingChatResponse, StreamingResponse, StreamingResponseSender,
    },
    request::{parse_response_json, response_is_sse, send_standard_request},
    streaming::{stream_sse_to_channel, StreamingChunkMapper},
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
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>> {
        send_openai_request(
            &self.client,
            &self.url,
            None,
            self.token.as_deref(),
            chunk_tx,
            &ChatRequestTransformation {
                supports_message_name: false,
                system_in_messages: true,
                strip_model_prefix: Some("openai/".into()),
            },
            options,
        )
        .await?;
        Ok(())
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
    chunk_tx: StreamingResponseSender,
    transform: &ChatRequestTransformation<'_>,
    SendRequestOptions {
        override_url,
        timeout,
        api_key,
        mut body,
    }: SendRequestOptions,
) -> Result<(), Report<ProviderError>> {
    body.transform(transform);

    if body.stream {
        // Enable usage when in streaming mode.
        body.stream_options = Some(StreamOptions {
            include_usage: true,
        });
    }

    let bytes = serde_json::to_vec(&body)
        .change_context_lazy(|| ProviderError::from_kind(ProviderErrorKind::TransformingRequest))?;
    let bytes = Bytes::from(bytes);

    let token = api_key
        .as_deref()
        .or(provider_token)
        // Allow no API key since we could be sending to an internal OpenAI-compatible service.
        .unwrap_or_default();

    let streaming = body.stream;
    let start_time = tokio::time::Instant::now();
    let (response, latency) = send_standard_request(
        timeout,
        || {
            let req = client
                .post(override_url.as_deref().unwrap_or(url))
                .bearer_auth(token)
                .header(CONTENT_TYPE, "application/json; charset=utf8")
                .headers(headers.cloned().unwrap_or_default());

            if streaming {
                req.header(ACCEPT, "text/event-stream")
            } else {
                req
            }
        },
        handle_rate_limit_headers,
        bytes,
    )
    .await?;

    if response_is_sse(&response) {
        let processor = StreamingEventProcessor { start_time };
        stream_sse_to_channel(response, chunk_tx, processor);
    } else {
        let result = parse_response_json::<SingleChatResponse>(response, latency).await;

        match result {
            Ok(result) => {
                let model = result.model.clone().or(body.model).unwrap_or_default();
                let response = StreamingResponse::Single(result);
                let info = StreamingResponse::ResponseInfo(ResponseInfo { model, meta: None });
                chunk_tx.send_async(Ok(response)).await.ok();
                chunk_tx.send_async(Ok(info)).await.ok();
            }
            Err(e) => {
                chunk_tx.send_async(Err(e)).await.ok();
            }
        }
    }

    Ok(())
}

struct StreamingEventProcessor {
    start_time: tokio::time::Instant,
}

impl StreamingChunkMapper for StreamingEventProcessor {
    fn process_chunk(
        &mut self,
        event: &Event,
    ) -> Result<Option<StreamingChatResponse>, Report<ProviderError>> {
        if event.data == "[DONE]" {
            return Ok(None);
        }

        if event.event == "error" {
            Err(Report::new(ProviderError {
                kind: ProviderErrorKind::Generic,
                status_code: None,
                body: serde_json::from_str(&event.data).ok(),
                latency: self.start_time.elapsed(),
            }))
        } else {
            serde_json::from_str::<StreamingChatResponse>(&event.data)
                .map(Some)
                .change_context_lazy(|| ProviderError {
                    kind: ProviderErrorKind::ParsingResponse,
                    status_code: None,
                    body: serde_json::from_str(&event.data).ok(),
                    latency: self.start_time.elapsed(),
                })
        }
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wiremock::MockServer;

    use super::*;
    use crate::testing::test_fixture_response;

    async fn run_fixture_test(test_name: &str, stream: bool, response: &str) {
        let server = MockServer::start().await;
        let mut provider = super::OpenAi::new(reqwest::Client::new(), Some("token".to_string()));
        provider.url = format!("{}/v1/chat_completions", server.uri());

        let provider = Arc::new(provider) as Arc<dyn ChatModelProvider>;
        test_fixture_response(
            test_name,
            server,
            "/v1/chat_completions",
            provider,
            stream,
            response,
        )
        .await
    }

    #[tokio::test]
    async fn text_streaming() {
        run_fixture_test(
            "openai_text_streaming",
            true,
            include_str!("./fixtures/openai_text_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn text_nonstreaming() {
        run_fixture_test(
            "openai_text_nonstreaming",
            false,
            include_str!("./fixtures/openai_text_response_nonstreaming.json"),
        )
        .await
    }

    #[tokio::test]
    async fn tool_calls_streaming() {
        run_fixture_test(
            "openai_tool_calls_streaming",
            true,
            include_str!("./fixtures/openai_tools_response_streaming.txt"),
        )
        .await
    }

    #[tokio::test]
    async fn tool_calls_nonstreaming() {
        run_fixture_test(
            "openai_tool_calls_nonstreaming",
            false,
            include_str!("./fixtures/openai_tools_response_nonstreaming.json"),
        )
        .await
    }
}
