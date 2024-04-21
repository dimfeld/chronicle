use std::time::Duration;

use error_stack::Report;
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use super::{ChatModelProvider, ProviderResponse};
use crate::{format::ChatRequest, request::RetryOptions, Error};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomProvider {
    pub name: String,
    pub url: String,
    pub format: ProviderRequestFormat,
    /// A list of models that this provider should be the default for
    pub default_for: Option<Vec<String>>,
}

/// The format that this proider uses for requests
/// todo move this somewhere else
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRequestFormat {
    OpenAi,
}

#[async_trait::async_trait]
impl ChatModelProvider for CustomProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send_request(
        &self,
        retry_options: RetryOptions,
        timeout: Duration,
        body: ChatRequest,
    ) -> Result<ProviderResponse, Report<Error>> {
        // https://docs.anthropic.com/claude/reference/messages_post
        todo!()
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        self.default_for
            .as_ref()
            .map(|v| v.iter().any(|s| s.as_str() == model))
            .unwrap_or(false)
    }
}
