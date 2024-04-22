use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use reqwest::{header::CONTENT_TYPE, Response};
use tracing::instrument;

use super::{
    openai::handle_rate_limit_headers, ChatModelProvider, ProviderResponse, SendRequestOptions,
};
use crate::{
    format::{ChatRequest, ChatRequestTransformation},
    request::{send_standard_request, RetryOptions},
    Error, ProxyRequestOptions,
};

#[derive(Debug)]
pub struct Groq {
    client: reqwest::Client,
    token: Option<String>,
}

#[async_trait::async_trait]
impl ChatModelProvider for Groq {
    fn name(&self) -> &str {
        "Groq"
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
        body.transform(ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: true,
            strip_model_prefix: Some("groq/"),
        });

        // Groq prohibits sending these fields
        body.logprobs = None;
        body.logit_bias = None;
        body.top_logprobs = None;
        body.n = None;

        let body = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let body = Bytes::from(body);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or(Error::MissingApiKey)?;

        let result = send_standard_request(
            retry_options,
            timeout,
            || {
                self.client
                    .post("https://api.groq.com/openai/v1/chat/completions")
                    .bearer_auth(api_token)
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            handle_rate_limit_headers,
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
        model.starts_with("groq/")
    }
}
