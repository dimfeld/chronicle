use serde::{Deserialize, Serialize};

use super::ChatModelProvider;

#[derive(Debug)]
pub struct OpenAi {}

#[async_trait::async_trait]
impl ChatModelProvider for OpenAi {
    fn name(&self) -> &str {
        "OpenAI"
    }

    async fn send_request(
        &self,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        // https://platform.openai.com/docs/api-reference/chat
        todo!()
    }

    fn default_url(&self) -> &'static str {
        todo!()
    }

    fn is_default_for_model(&self, model: &str) -> bool {
        model.starts_with("gpt-")
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Usage {
    pub prompt_tokens: Option<usize>,
    pub completion_tokens: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MessageResult {
    pub message: ChatMessage,
    pub finish_reason: String,
    pub usage: Usage,
}

pub struct ChatRequest {
    max_tokens: Option<usize>,
    temperature: Option<f32>,
    /// The system prompt to use
    system: Option<String>,
    messages: Vec<ChatMessage>,
}
