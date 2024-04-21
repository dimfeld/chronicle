use std::time::Duration;

use error_stack::Report;

use super::{ChatModelProvider, ProviderResponse};
use crate::{format::ChatRequest, request::RetryOptions, Error};

#[derive(Debug)]
pub struct Groq {
    pub url: String,
}

#[async_trait::async_trait]
impl ChatModelProvider for Groq {
    fn name(&self) -> &str {
        "Groq"
    }

    async fn send_request(
        &self,
        retry_options: RetryOptions,
        timeout: Duration,
        body: ChatRequest,
    ) -> Result<ProviderResponse, Report<Error>> {
        todo!()
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("groq/")
    }
}
