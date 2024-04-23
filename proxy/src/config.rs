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
    /// The model to use
    pub model: String,
    /// The provider to use
    pub provider: String,
    /// An API key configuration to use
    pub api_key_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct ApiKeyConfig {
    /// A name for this key
    pub name: String,
    /// If "env", the key is an environment variable name to read, rather than the key itself.
    /// Eventually this will support other pluggable sources.
    pub source: String,
    /// The key itself, or if `from_env` is set, the name of the environment variable to read.
    pub value: String,
}

impl std::fmt::Debug for ApiKeyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyConfig")
            .field("name", &self.name)
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
    pub token: Option<String>,
    pub format: ProviderRequestFormat,
    /// Extra headers to pass with the request
    pub headers: BTreeMap<String, String>,
    pub prefix: Option<String>,
    /// A list of models that this provider should be the default for
    #[serde(default)]
    pub default_for: Vec<String>,
    pub token_env: Option<String>,
}

impl CustomProviderConfig {
    pub fn into_provider(mut self, client: reqwest::Client) -> CustomProvider {
        if let Some(token) = self
            .token_env
            .as_deref()
            .and_then(|var| std::env::var(&var).ok())
        {
            self.token = Some(token);
        }

        CustomProvider::new(self, client)
    }
}
