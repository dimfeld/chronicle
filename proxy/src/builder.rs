use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use error_stack::{Report, ResultExt};

use crate::{
    config::{AliasConfig, ApiKeyConfig, CustomProviderConfig, ProxyConfig},
    database::{
        load_aliases_from_database, load_api_key_configs_from_database,
        load_providers_from_database, logging::start_database_logger, Pool,
    },
    providers::{
        anthropic::Anthropic, groq::Groq, ollama::Ollama, openai::OpenAi, ChatModelProvider,
    },
    Error, ProviderLookup, Proxy,
};

pub struct ProxyBuilder {
    pool: Option<Pool>,
    config: ProxyConfig,
    openai: Option<String>,
    ollama: Option<String>,
    anthropic: Option<String>,
    groq: Option<String>,
    client: Option<reqwest::Client>,
}

impl ProxyBuilder {
    pub fn new() -> Self {
        Self {
            pool: None,
            config: ProxyConfig::default(),
            openai: Some(String::new()),
            anthropic: Some(String::new()),
            groq: Some(String::new()),
            ollama: Some(String::new()),
            client: None,
        }
    }

    /// Set the database connection pool
    pub fn with_database(mut self, pool: Pool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Enable or disable logging to the database. Logging requires `with_database` to have been
    /// called.
    pub fn log_to_database(mut self, log: bool) -> Self {
        self.config.log_to_database = Some(log);
        self
    }

    /// Merge this configuration into the current one.
    pub fn with_config(mut self, config: ProxyConfig) -> Self {
        self.config.default_timeout = config.default_timeout.or(self.config.default_timeout);
        self.config.log_to_database = config.log_to_database.or(self.config.log_to_database);
        if config.user_agent.is_some() {
            self.config.user_agent = config.user_agent;
        }
        self.config.providers.extend(config.providers);
        self.config.aliases.extend(config.aliases);
        self.config.api_keys.extend(config.api_keys);
        self
    }

    /// Read a configuration file from this path and merge it into the current configuration.
    pub async fn with_config_from_path(self, path: &Path) -> Result<Self, Report<Error>> {
        let data = tokio::fs::read_to_string(path)
            .await
            .change_context(Error::ReadingConfig)?;
        let config: ProxyConfig = toml::from_str(&data).change_context(Error::ReadingConfig)?;

        Ok(self.with_config(config))
    }

    pub fn with_alias(mut self, alias: AliasConfig) -> Self {
        self.config.aliases.push(alias);
        self
    }

    pub fn with_api_key(mut self, key: ApiKeyConfig) -> Self {
        self.config.api_keys.push(key);
        self
    }

    /// Add a custom provider to the list of providers
    pub fn with_custom_provider(mut self, config: CustomProviderConfig) -> Self {
        self.config.providers.push(config);
        self
    }

    /// Enable the OpenAI provider, if it was disabled by [without_default_providers]
    pub fn with_openai(mut self, token: Option<String>) -> Self {
        self.openai = token.or(Some(String::new()));
        self
    }

    /// Enable the Anthropic provider, if it was disabled by [without_default_providers]
    pub fn with_anthropic(mut self, token: Option<String>) -> Self {
        self.anthropic = token.or(Some(String::new()));
        self
    }

    /// Enable the Groq provider, if it was disabled by [without_default_providers]
    pub fn with_groq(mut self, token: Option<String>) -> Self {
        self.groq = token.or(Some(String::new()));
        self
    }

    /// Enable the Ollama provider, if it was disabled by [without_default_providers]
    pub fn with_ollama(mut self, url: Option<String>) -> Self {
        self.ollama = url.or(Some(String::new()));
        self
    }

    /// Don't load the default providers
    pub fn without_default_providers(mut self) -> Self {
        self.anthropic = None;
        self.groq = None;
        self.openai = None;
        self.ollama = None;
        self
    }

    /// Set the user agent that will be used for HTTP requests. This only applies if
    /// `with_client` is not used.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.config.user_agent = Some(user_agent.into());
        self
    }

    /// Supply a custom [reqwest::Client]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    pub async fn build(self) -> Result<Proxy, Report<Error>> {
        let mut providers = self.config.providers;
        let mut api_keys = self.config.api_keys;
        let mut aliases = self.config.aliases;
        let logger = if let Some(pool) = &self.pool {
            let db_providers = load_providers_from_database(&pool).await?;
            let db_aliases = load_aliases_from_database(&pool).await?;
            let db_api_keys = load_api_key_configs_from_database(&pool).await?;

            providers.extend(db_providers);
            aliases.extend(db_aliases);
            api_keys.extend(db_api_keys);

            let logger = if self.config.log_to_database.unwrap_or(false) {
                Some(start_database_logger(
                    pool.clone(),
                    500,
                    Duration::from_secs(1),
                ))
            } else {
                None
            };

            logger
        } else {
            None
        };

        let client = self.client.unwrap_or_else(|| {
            reqwest::Client::builder()
                .user_agent(self.config.user_agent.as_deref().unwrap_or("chronicle"))
                .timeout(
                    self.config
                        .default_timeout
                        .unwrap_or(Duration::from_secs(60)),
                )
                .build()
                .unwrap()
        });

        let mut providers = providers
            .into_iter()
            .map(|c| Arc::new(c.into_provider(client.clone())) as Arc<dyn ChatModelProvider>)
            .collect::<Vec<_>>();

        fn empty_to_none(s: String) -> Option<String> {
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }

        if let Some(token) = self.anthropic {
            providers.push(
                Arc::new(Anthropic::new(client.clone(), empty_to_none(token)))
                    as Arc<dyn ChatModelProvider>,
            );
        }

        if let Some(token) = self.openai {
            providers.push(Arc::new(OpenAi::new(client.clone(), empty_to_none(token)))
                as Arc<dyn ChatModelProvider>);
        }

        if let Some(token) = self.groq {
            providers.push(Arc::new(Groq::new(client.clone(), empty_to_none(token)))
                as Arc<dyn ChatModelProvider>);
        }

        if let Some(url) = self.ollama {
            providers.push(Arc::new(Ollama::new(client.clone(), empty_to_none(url)))
                as Arc<dyn ChatModelProvider>);
        }

        let (log_tx, log_task) = logger.unzip();

        let api_keys = api_keys
            .into_iter()
            .map(|mut config| {
                if config.source == "env" {
                    let value = std::env::var(&config.value).map_err(|_| {
                        Error::MissingApiKeyEnv(config.name.clone(), config.value.clone())
                    })?;

                    config.value = value;
                }

                Ok::<_, Error>(config)
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let lookup = ProviderLookup {
            providers,
            aliases,
            api_keys,
        };

        Ok(Proxy {
            pool: self.pool,
            lookup: RwLock::new(lookup),
            default_timeout: self.config.default_timeout,
            log_tx,
            log_task,
        })
    }
}
