use super::custom::{CustomProvider, OpenAiRequestFormatOptions, ProviderRequestFormat};
use crate::config::CustomProviderConfig;

pub struct Together;

impl Together {
    pub fn new(client: reqwest::Client, token: Option<String>) -> CustomProvider {
        let config = CustomProviderConfig {
            name: "together".into(),
            label: Some("together.ai".to_string()),
            url: "https://api.together.xyz/v1/chat/completions".into(),
            format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
            api_key: None,
            api_key_source: None,
            headers: Default::default(),
            prefix: Some("together/".to_string()),
        }
        .with_token_or_env(token, "TOGETHER_API_KEY");

        CustomProvider::new(config, client)
    }
}
