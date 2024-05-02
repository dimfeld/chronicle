use super::custom::{CustomProvider, OpenAiRequestFormatOptions, ProviderRequestFormat};
use crate::config::CustomProviderConfig;

pub struct DeepInfra;

impl DeepInfra {
    pub fn new(client: reqwest::Client, token: Option<String>) -> CustomProvider {
        let config = CustomProviderConfig {
            name: "deepinfra".into(),
            label: Some("DeepInfra".to_string()),
            url: "https://api.deepinfra.com/v1/openai/chat/completions".into(),
            format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
            api_key: None,
            api_key_source: None,
            headers: Default::default(),
            prefix: Some("deepinfra/".to_string()),
        }
        .with_token_or_env(token, "DEEPINFRA_API_KEY");

        CustomProvider::new(config, client)
    }
}
