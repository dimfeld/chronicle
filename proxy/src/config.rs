use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

use crate::providers::custom::{CustomProvider, ProviderRequestFormat};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProxyConfig {
    #[serde(default)]
    pub providers: Vec<CustomProviderConfig>,
    #[serde(default)]
    pub aliases: Vec<AliasConfig>,
    #[serde(default)]
    pub api_keys: Vec<ApiKeyConfig>,
    pub default_timeout: Option<Duration>,
    pub log_to_database: Option<bool>,
    pub user_agent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct AliasConfig {
    /// A name for this model-provider pair.
    pub name: String,
    /// If true, start from a random provider.
    /// If false, always start with the first provider, and only use later providers on retry.
    #[serde(default)]
    pub random_order: bool,
    pub models: Vec<AliasConfigProvider>,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct AliasConfigProvider {
    /// The model to use
    pub model: String,
    /// The provider to use
    pub provider: String,
    /// An API key configuration to use
    pub api_key_name: Option<String>,
}

sqlx_transparent_json_decode::sqlx_json_decode!(AliasConfigProvider);

#[derive(Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ApiKeyConfig {
    /// A name for this key
    pub name: String,
    /// If "env", the key is an environment variable name to read, rather than the key itself.
    /// Eventually this will support other pluggable sources.
    pub source: String,
    /// The key itself, or if `source` is "env", the name of the environment variable to read.
    pub value: String,
}

impl std::fmt::Debug for ApiKeyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyConfig")
            .field("name", &self.name)
            .field("source", &self.source)
            .field(
                "value",
                if self.source == "env" {
                    &self.value
                } else {
                    &"***"
                },
            )
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomProviderConfig {
    pub name: String,
    pub label: Option<String>,
    /// The url to use
    pub url: String,
    /// The API token to pass along
    pub api_key: Option<String>,
    /// Where to retrieve the value for `api_key`.
    /// If `api_key_source` is "env" then `api_key` is an environment variable.
    /// If it is empty, then `api_key` is assumed to be the token itself, if provided.
    pub api_key_source: Option<String>,
    pub format: ProviderRequestFormat,
    /// Extra headers to pass with the request
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Models starting with this prefix will use this provider by default.
    pub prefix: Option<String>,
}

impl CustomProviderConfig {
    pub fn into_provider(mut self, client: reqwest::Client) -> CustomProvider {
        if self.api_key_source.as_deref().unwrap_or_default() == "env" {
            if let Some(token) = self
                .api_key
                .as_deref()
                .and_then(|var| std::env::var(&var).ok())
            {
                self.api_key = Some(token);
            }
        }

        CustomProvider::new(self, client)
    }
}
