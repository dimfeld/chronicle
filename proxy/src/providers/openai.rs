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
        mut body: ChatRequest,
    ) -> Result<ProviderResponse, Report<Error>> {
        body.transform(ChatRequestTransformation {
            supports_message_name: false,
            system_in_messages: true,
        });

        let body = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let body = Bytes::from(body);

        let result = send_standard_request(
            retry_options,
            || self.client.post(&self.url).bearer_auth(&self.token),
            |res| {
                res.headers()
                    // TODO This is not the right code, need to look up how it works in the
                    // documentation
                    .get("x-retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|value| Duration::from_millis(value))
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
        model.starts_with("gpt-")
    }
}
