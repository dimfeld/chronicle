use super::custom::{CustomProvider, OpenAiRequestFormatOptions, ProviderRequestFormat};
use crate::config::CustomProviderConfig;

pub struct Anyscale;

impl Anyscale {
    pub fn new(client: reqwest::Client, token: Option<String>) -> CustomProvider {
        let config = CustomProviderConfig {
            name: "anyscale".into(),
            label: Some("Anyscale".to_string()),
            url: "https://api.endpoints.anyscale.com/v1/chat/completions".into(),
            format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions::default()),
            api_key: None,
            api_key_source: None,
            headers: Default::default(),
            prefix: Some("anyscale/".to_string()),
        }
        .with_token_or_env(token, "ANYSCALE_API_KEY");

        CustomProvider::new(config, client)
    }
}
