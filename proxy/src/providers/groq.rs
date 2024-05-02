use bytes::Bytes;
use error_stack::{Report, ResultExt};
use reqwest::header::CONTENT_TYPE;

use super::{
    openai::handle_rate_limit_headers, ChatModelProvider, ProviderResponse, SendRequestOptions,
};
use crate::{
    format::{ChatRequestTransformation, ChatResponse},
    request::send_standard_request,
    Error,
};

#[derive(Debug)]
pub struct Groq {
    client: reqwest::Client,
    token: Option<String>,
}

impl Groq {
    pub fn new(client: reqwest::Client, token: Option<String>) -> Self {
        Self {
            client,
            token: token.or_else(|| std::env::var("GROQ_API_KEY").ok()),
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for Groq {
    fn name(&self) -> &str {
        "groq"
    }

    fn label(&self) -> &str {
        "Groq"
    }

    async fn send_request(
        &self,
        SendRequestOptions {
            override_url,
            timeout,
            api_key,
            mut body,
        }: SendRequestOptions,
    ) -> Result<ProviderResponse, Report<Error>> {
        body.transform(&ChatRequestTransformation {
            supports_message_name: true,
            system_in_messages: true,
            strip_model_prefix: Some("groq/".into()),
        });

        // Groq prohibits sending these fields
        body.logprobs = None;
        body.logit_bias = None;
        body.top_logprobs = None;
        body.n = None;

        let bytes = serde_json::to_vec(&body).change_context(Error::TransformingRequest)?;
        let bytes = Bytes::from(bytes);

        let api_token = api_key
            .as_deref()
            .or(self.token.as_deref())
            .ok_or(Error::MissingApiKey)?;

        let result = send_standard_request::<ChatResponse>(
            timeout,
            || {
                self.client
                    .post(
                        override_url
                            .as_deref()
                            .unwrap_or("https://api.groq.com/openai/v1/chat/completions"),
                    )
                    .bearer_auth(api_token)
                    .header(CONTENT_TYPE, "application/json; charset=utf8")
            },
            handle_rate_limit_headers,
            bytes,
        )
        .await
        .change_context(Error::ModelError)?;

        Ok(ProviderResponse {
            model: result.0.model.clone().or(body.model).unwrap_or_default(),
            body: result.0,
            latency: result.1,
            meta: None,
        })
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("groq/")
    }
}
