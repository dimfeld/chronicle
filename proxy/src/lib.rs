use std::{
    collections::BTreeMap,
    fmt::Debug,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};

pub mod database;
pub mod error;
pub mod format;
pub mod providers;
pub mod request;

use database::{load_providers_from_database, logging::ProxyLogEntry, Pool};
pub use error::Error;
use error_stack::{Report, ResultExt};
use format::{ChatRequest, ChatResponse};
use providers::{
    custom::{CustomProvider, ProviderRequestFormat},
    openai::OpenAi,
    ChatModelProvider,
};
use request::RetryOptions;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    database::logging::start_database_logger,
    providers::{anthropic::Anthropic, groq::Groq, SendRequestOptions},
};

pub type AnyChatModelProvider = Arc<dyn ChatModelProvider>;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ProxyConfig {
    providers: Vec<CustomProviderConfig>,
    default_timeout: Option<Duration>,
    log_to_database: Option<bool>,
    user_agent: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomProviderConfig {
    pub name: String,
    pub url: String,
    pub token: Option<String>,
    pub format: ProviderRequestFormat,
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

        CustomProvider {
            config: self,
            client,
        }
    }
}

pub struct ProxyBuilder {
    pool: Option<Pool>,
    config: ProxyConfig,
    openai: Option<String>,
    anthropic: Option<String>,
    groq: Option<String>,
    client: Option<reqwest::Client>,
}

impl ProxyBuilder {
    pub fn new() -> Self {
        Self {
            pool: None,
            config: ProxyConfig::default(),
            openai: None,
            anthropic: None,
            groq: None,
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

    /// Add a custom provider to the list of providers
    pub fn with_custom_provider(mut self, config: CustomProviderConfig) -> Self {
        self.config.providers.push(config);
        self
    }

    /// Enable the OpenAI provider
    pub fn with_openai(mut self, token: Option<String>) -> Self {
        self.openai = token.or(Some(String::new()));
        self
    }

    /// Enable the Anthropic provider
    pub fn with_anthropic(mut self, token: Option<String>) -> Self {
        self.anthropic = token.or(Some(String::new()));
        self
    }

    /// Enable the Groq provider
    pub fn with_groq(mut self, token: Option<String>) -> Self {
        self.groq = token.or(Some(String::new()));
        self
    }

    // /// Enable the Ollama provider
    // pub fn with_ollama(mut self, url: Option<String>) -> Self {
    //     self.ollama = url.or(Some(String::new()));
    //     self
    // }

    /// Load all the default providers
    pub fn with_default_providers(self) -> Self {
        self.with_anthropic(None).with_groq(None).with_openai(None)
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
        let logger = if let Some(pool) = &self.pool {
            let db_providers = load_providers_from_database(&pool).await?;
            providers.extend(db_providers);

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

        let (log_tx, log_task) = logger.unzip();

        Ok(Proxy {
            pool: self.pool,
            providers: RwLock::new(providers),
            default_timeout: self.config.default_timeout,
            log_tx,
            log_task,
        })
    }
}

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    log_tx: Option<flume::Sender<ProxyLogEntry>>,
    log_task: Option<tokio::task::JoinHandle<()>>,
    providers: RwLock<Vec<Arc<dyn ChatModelProvider>>>,
    default_timeout: Option<Duration>,
}

impl Proxy {
    pub fn builder() -> ProxyBuilder {
        ProxyBuilder::new()
    }

    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .read()
            .unwrap()
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
    }

    pub fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .read()
            .unwrap()
            .iter()
            .find(|p| p.is_default_for_model(model))
            .map(Arc::clone)
    }

    fn model_from_options<'a>(
        options: &'a ProxyRequestOptions,
        body: &'a ChatRequest,
    ) -> Result<(bool, &'a str), Error> {
        let (from_options, model) = if let Some(model) = &options.model {
            (true, model.as_str())
        } else {
            (false, body.model.as_deref().unwrap_or_default())
        };

        if model.is_empty() {
            Err(Error::ModelNotSpecified)
        } else {
            Ok((from_options, model))
        }
    }

    /// Send a request, choosing the provider based on the requested `model` and `provider`.
    ///
    /// `options.provider` can be used to choose a specific provider.
    /// `options.model` will be used next to choose a model to use
    /// `body["model"]` is used if options.model is empty.
    #[instrument]
    pub async fn send(
        &self,
        options: ProxyRequestOptions,
        body: ChatRequest,
    ) -> Result<ChatResponse, Report<Error>> {
        let provider = if let Some(provider) = &options.provider {
            self.get_provider(&provider)
                .ok_or(Error::UnknownProvider(provider.clone()))?
        } else {
            let (_, model) = Self::model_from_options(&options, &body)?;
            self.default_provider_for_model(model)
                .ok_or_else(|| Error::NoDefault(model.to_string()))?
        };

        self.send_to_provider(provider, options, body).await
    }

    /// Send a request to a provider
    #[instrument(fields(
        latency,
        total_latency,
        retries,
        rate_limited,
        tokens_input,
        tokens_output,
        status_code
    ))]
    pub async fn send_to_provider(
        &self,
        provider: Arc<dyn ChatModelProvider>,
        options: ProxyRequestOptions,
        mut body: ChatRequest,
    ) -> Result<ChatResponse, Report<Error>> {
        tracing::info!(?body, "Starting request");
        // Send update to postgres recorder

        let (from_options, model) = Self::model_from_options(&options, &body)?;

        // If we got the model from the options, then overwrite the model in the body
        if from_options {
            body.model = Some(model.to_string());
        }

        let timestamp = chrono::Utc::now();
        let send_start = tokio::time::Instant::now();
        let response = provider
            .send_request(SendRequestOptions {
                retry_options: options.retry.clone(),
                timeout: options
                    .timeout
                    .or(self.default_timeout)
                    .unwrap_or(Duration::from_secs(60)),
                api_key: options.api_key.clone(),
                body: body.clone(),
            })
            .await;
        let send_time = send_start.elapsed().as_millis();

        // Get response stats: latency, tokens used, etc.
        // We want to record both the total latency and the latency of the final working request
        // Hopefully we can do that in a way that allows Postgres to use a HOT update
        // todo better tracing here
        // todo send the response stats to the postgres recorder
        let current_span = tracing::Span::current();
        // In case of retries, this might be meaningfully different from the main latency.
        current_span.record("total_latency", send_time);

        match &response {
            Ok(response) => {
                current_span.record("latency", response.latency.as_millis());
                current_span.record("retries", response.retries);
                current_span.record("rate_limited", response.rate_limited);
                if let Some(input_tokens) = response.body.usage.prompt_tokens {
                    current_span.record("tokens_input", input_tokens);
                }
                if let Some(output_tokens) = response.body.usage.completion_tokens {
                    current_span.record("tokens_output", output_tokens);
                }

                if let Some(log_tx) = &self.log_tx {
                    let log_entry = ProxyLogEntry {
                        timestamp,
                        request: body.clone(),
                        response: Some(response.clone()),
                        total_latency: send_start.elapsed(),
                        error: None,
                        options,
                    };

                    log_tx.send_async(log_entry).await.ok();
                }
            }
            Err(e) => {
                tracing::error!(?e, "Request failed");

                if let Some(log_tx) = &self.log_tx {
                    let log_entry = ProxyLogEntry {
                        timestamp,
                        request: body,
                        response: None,
                        total_latency: send_start.elapsed(),
                        error: Some(format!("{:?}", e)),
                        options,
                    };

                    log_tx.send_async(log_entry).await.ok();
                }
            }
        }

        response.map(|r| r.body)
    }

    pub async fn shutdown(&mut self) {
        let log_tx = self.log_tx.take();
        drop(log_tx);
        let log_task = self.log_task.take();
        if let Some(log_task) = log_task {
            log_task.await.ok();
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyRequestOptions {
    /// Override the model from the request body.
    pub model: Option<String>,
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub timeout: Option<std::time::Duration>,
    pub retry: RetryOptions,

    pub metadata: ProxyRequestMetadata,
    pub internal_metadata: ProxyRequestInternalMetadata,
}

impl Default for ProxyRequestOptions {
    fn default() -> Self {
        Self {
            model: None,
            provider: None,
            api_key: None,
            retry: RetryOptions::default(),
            timeout: None,
            metadata: ProxyRequestMetadata::default(),
            internal_metadata: ProxyRequestInternalMetadata::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
/// Metadata about the internal source of this request. Mostly useful for multi-tenant
/// scenarios where one proxy server is handling requests from multiple unrelated applications.
pub struct ProxyRequestInternalMetadata {
    /// The internal organiztion that the request belongs to
    pub organization_id: Option<String>,
    /// The internal project that the request belongs to
    pub project_id: Option<String>,
    /// The internal user ID that the request belongs to
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
/// Metadata about the request and how it fits into the system as a whole. All of these
/// fields are optional, and the `extra` field can be used to add anything else that useful
/// for your use case.
pub struct ProxyRequestMetadata {
    /// The application making this call
    pub application: Option<String>,
    /// The environment the application is running in
    pub environment: Option<String>,
    /// The organization related to the request
    pub organization_id: Option<String>,
    /// The project related to the request
    pub project_id: Option<String>,
    /// The id of the user that triggered the request
    pub user_id: Option<String>,
    /// The id of the workflow that this request belongs to
    pub workflow_id: Option<String>,
    /// A readable name of the workflow that this request belongs to
    pub workflow_name: Option<String>,
    /// The id of of the specific run that this request belongs to.
    pub run_id: Option<String>,
    /// The name of the workflow step
    pub step: Option<String>,
    /// The index of the step within the workflow.
    pub step_index: Option<u32>,

    /// Any other metadata to include.
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::ProxyRequestMetadata;

    #[test]
    /// Make sure extra flattening works as expected
    fn deserialize_meta() {
        let test_value = json!({
            "application": "abc",
            "another": "value",
            "step": "email",
            "third": "fourth",
        });

        let value: ProxyRequestMetadata =
            serde_json::from_value(test_value).expect("deserializing");

        println!("{value:#?}");
        assert_eq!(value.application, Some("abc".to_string()));
        assert_eq!(value.step, Some("email".to_string()));
        assert_eq!(
            value.extra.as_ref().unwrap().get("another").unwrap(),
            &json!("value")
        );
        assert_eq!(
            value.extra.as_ref().unwrap().get("third").unwrap(),
            &json!("fourth")
        );
    }
}
