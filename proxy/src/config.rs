use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

use crate::providers::custom::{CustomProvider, ProviderRequestFormat};

/// Configuration for the proxy
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProxyConfig {
    /// Model providers that the proxy should use
    #[serde(default)]
    pub providers: Vec<CustomProviderConfig>,
    /// Aliases that map to providers and models
    #[serde(default)]
    pub aliases: Vec<AliasConfig>,
    /// API keys that the proxy should use
    #[serde(default)]
    pub api_keys: Vec<ApiKeyConfig>,
    /// The default timeout for requests
    pub default_timeout: Option<Duration>,
    /// Whether to log to the database or not.
    pub log_to_database: Option<bool>,
    /// The user agent to use when making requests
    pub user_agent: Option<String>,
}

/// An alias configuration mape a single name to a list of provider-model pairs
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct AliasConfig {
    /// A name for this alias
    pub name: String,
    /// If true, start from a random provider.
    /// If false, always start with the first provider, and only use later providers on retry.
    #[serde(default)]
    pub random_order: bool,
    /// The providers and models that this alias represents.
    pub models: Vec<AliasConfigProvider>,
}

/// A provider and model to use in an alias
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

/// An API key, or where to find one
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

/// A declarative definition of a model provider
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomProviderConfig {
    /// The name of the provider, as referenced in proxy requests
    pub name: String,
    /// A human-readable name for the provider
    pub label: Option<String>,
    /// The url to use
    pub url: String,
    /// The API token to pass along
    pub api_key: Option<String>,
    /// Where to retrieve the value for `api_key`.
    /// If `api_key_source` is "env" then `api_key` is an environment variable.
    /// If it is empty, then `api_key` is assumed to be the token itself, if provided.
    /// In the future the key sources will be pluggable, to support external secret sources.
    pub api_key_source: Option<String>,
    /// What kind of request format this provider uses. Defaults to OpenAI-compatible
    #[serde(default)]
    pub format: ProviderRequestFormat,
    /// Extra headers to pass with the request
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Models starting with this prefix will use this provider by default.
    pub prefix: Option<String>,
}

impl CustomProviderConfig {
    /// Generate a [CustomProvider] object from the configuration
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

    /// Add an API token to the [CustomProviderConfig], or if one is not provided, then configure
    /// it to read from the given environment variable.
    pub fn with_token_or_env(mut self, token: Option<String>, env: &str) -> Self {
        match token {
            Some(token) => {
                self.api_key = Some(token);
                self.api_key_source = None;
            }
            None => {
                self.api_key = Some(env.to_string());
                self.api_key_source = Some("env".to_string());
            }
        }

        self
    }
}
