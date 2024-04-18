pub mod anthropic;
pub mod custom;
pub mod ollama;
pub mod openai;

#[async_trait::async_trait]
pub trait ChatModelProvider: Debug + Send + Sync {
    fn name(&self) -> &str;

    async fn send_request(
        &self,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error>;

    fn default_url(&self) -> &'static str;

    fn is_default_for_model(&self, model: &str) -> bool;
}
