use super::custom::{CustomProvider, OpenAiRequestFormatOptions, ProviderRequestFormat};
use crate::config::CustomProviderConfig;

pub struct Fireworks;

impl Fireworks {
    pub fn new(client: reqwest::Client, token: Option<String>) -> CustomProvider {
        let config = CustomProviderConfig {
            name: "fireworks".into(),
            label: Some("fireworks.ai".to_string()),
            url: "https://api.fireworks.ai/inference/v1/chat/completions".into(),
            format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
            api_key: None,
            api_key_source: None,
            headers: Default::default(),
            prefix: Some("fireworks/".to_string()),
        }
        .with_token_or_env(token, "FIREWORKS_API_KEY");

        CustomProvider::new(config, client)
    }
}
