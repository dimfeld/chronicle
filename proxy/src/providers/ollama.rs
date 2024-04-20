use error_stack::Report;

use super::{ChatModelProvider, ProviderResponse};
use crate::{format::ChatRequest, request::RetryOptions, Error, ProxyRequestOptions};

#[derive(Debug)]
pub struct Ollama {
    pub url: String,
}

#[async_trait::async_trait]
impl ChatModelProvider for Ollama {
    fn name(&self) -> &str {
        "Ollama"
    }

    async fn send_request(
        &self,
        retry_options: RetryOptions,
        body: ChatRequest,
    ) -> Result<ProviderResponse, Report<Error>> {
        todo!()
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("claude")
    }
}
