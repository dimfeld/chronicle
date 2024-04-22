use std::{collections::BTreeMap, time::Duration};

use error_stack::Report;
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use super::{openai::send_openai_request, ChatModelProvider, ProviderResponse, SendRequestOptions};
use crate::{
    format::{ChatRequest, ChatRequestTransformation},
    request::RetryOptions,
    CustomProviderConfig, Error,
};

#[derive(Debug, Clone)]
pub struct CustomProvider {
    pub config: CustomProviderConfig,
    pub client: reqwest::Client,
}

/// The format that this proider uses for requests
/// todo move this somewhere else
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderRequestFormat {
    OpenAi {
        transforms: ChatRequestTransformation<'static>,
    },
}

#[async_trait::async_trait]
impl ChatModelProvider for CustomProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn send_request(
        &self,
        options: SendRequestOptions,
    ) -> Result<ProviderResponse, Report<Error>> {
        match &self.config.format {
            ProviderRequestFormat::OpenAi { transforms } => {
                send_openai_request(
                    &self.client,
                    &self.config.url,
                    self.config.token.as_deref(),
                    &transforms,
                    options,
                )
                .await
            }
        }
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        self.config
            .prefix
            .as_deref()
            .map(|s| model.starts_with(s))
            .unwrap_or(false)
            || self.config.default_for.iter().any(|s| s.as_str() == model)
    }
}
