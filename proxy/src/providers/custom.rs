use super::ChatModelProvider;

#[derive(Debug)]
pub struct CustomProvider {
    pub name: String,
    pub format: ProviderRequestFormat,
}

/// The format that this proider uses for requests
/// todo move this somewhere else
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
        body: serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        // https://docs.anthropic.com/claude/reference/messages_post
        todo!()
    }

    fn default_url(&self) -> &'static str {
        ""
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        false
    }
}
