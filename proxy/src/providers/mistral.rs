use super::custom::{CustomProvider, OpenAiRequestFormatOptions, ProviderRequestFormat};
use crate::config::CustomProviderConfig;

pub struct Mistral;

impl Mistral {
    pub fn new(client: reqwest::Client, token: Option<String>) -> CustomProvider {
        let config = CustomProviderConfig {
            name: "mistral".into(),
            label: Some("Mistral".to_string()),
            url: "https://api.mistral.ai/v1/chat/completions".into(),
            format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
            api_key: None,
            api_key_source: None,
            headers: Default::default(),
            prefix: Some("mistral/".to_string()),
        }
        .with_token_or_env(token, "MISTRAL_API_KEY");

        CustomProvider::new(config, client)
    }
}
