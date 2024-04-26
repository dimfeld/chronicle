//! Handle custom provider configurations that look close enough to an existing provider
//! that we can declare them in data.

use error_stack::Report;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use super::{openai::send_openai_request, ChatModelProvider, ProviderResponse, SendRequestOptions};
use crate::{config::CustomProviderConfig, format::ChatRequestTransformation, Error};

#[derive(Debug, Clone)]
pub struct CustomProvider {
    pub config: CustomProviderConfig,
    pub client: reqwest::Client,
    pub headers: HeaderMap,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct OpenAiRequestFormatOptions {
    pub transforms: ChatRequestTransformation<'static>,
}

/// The format that this proider uses for requests
/// todo move this somewhere else
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderRequestFormat {
    OpenAi(OpenAiRequestFormatOptions),
}

impl CustomProvider {
    pub fn new(mut config: CustomProviderConfig, client: reqwest::Client) -> Self {
        let headers = std::mem::take(&mut config.headers);
        let headers: HeaderMap = headers
            .into_iter()
            .filter_map(|(k, v)| {
                let k = HeaderName::from_bytes(k.as_bytes()).ok()?;
                let v = HeaderValue::from_str(v.as_str()).ok()?;
                Some((k, v))
            })
            .collect();
        Self {
            config,
            client,
            headers,
        }
    }
}

#[async_trait::async_trait]
impl ChatModelProvider for CustomProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn label(&self) -> &str {
        self.config.label.as_deref().unwrap_or(&self.config.name)
    }

    async fn send_request(
        &self,
        options: SendRequestOptions,
    ) -> Result<ProviderResponse, Report<Error>> {
        match &self.config.format {
            ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions { transforms }) => {
                send_openai_request(
                    &self.client,
                    &self.config.url,
                    Some(&self.headers),
                    self.config.api_key.as_deref(),
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
