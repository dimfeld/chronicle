use super::ChatModelProvider;

#[derive(Debug)]
pub struct Anthropic {}

#[async_trait::async_trait]
impl ChatModelProvider for Anthropic {
    fn name(&self) -> &str {
        "Anthropic"
    }

    async fn send_request(
        &self,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        todo!()
    }

    fn default_url(&self) -> &'static str {
        todo!()
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("claude")
    }
}
