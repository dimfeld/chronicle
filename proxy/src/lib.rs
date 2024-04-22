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

use database::{load_providers_from_database, Pool};
pub use error::Error;
use error_stack::{Report, ResultExt};
use format::{ChatRequest, ChatResponse};
use providers::{
    custom::{CustomProvider, ProviderRequestFormat},
    ChatModelProvider,
};
use request::RetryOptions;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::providers::SendRequestOptions;

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
    client: Option<reqwest::Client>,
}

impl ProxyBuilder {
    pub fn new() -> Self {
        Self {
            pool: None,
            config: ProxyConfig::default(),
            client: None,
        }
    }

    pub fn with_database(mut self, pool: Pool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn log_to_database(mut self, log: bool) -> Self {
        self.config.log_to_database = Some(log);
        self
    }

    pub fn with_config(mut self, config: ProxyConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn with_config_from_path(mut self, path: &Path) -> Result<Self, Report<Error>> {
        let data = tokio::fs::read_to_string(path)
            .await
            .change_context(Error::ReadingConfig)?;
        let config: ProxyConfig = toml::from_str(&data).change_context(Error::ReadingConfig)?;

        // Merge the new config into whatever was set before.

        self.config.default_timeout = config.default_timeout.or(self.config.default_timeout);
        self.config.log_to_database = config.log_to_database.or(self.config.log_to_database);
        if config.user_agent.is_some() {
            self.config.user_agent = config.user_agent;
        }
        self.config.providers = config.providers;
        Ok(self)
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = Some(client);
        self
    }

    pub async fn build(self) -> Result<Proxy, Report<Error>> {
        let mut providers = self.config.providers;
        if let Some(pool) = &self.pool {
            let db_providers = load_providers_from_database(&pool).await?;
            providers.extend(db_providers);
        }

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

        let providers = providers
            .into_iter()
            .map(|c| Arc::new(c.into_provider(client.clone())) as Arc<dyn ChatModelProvider>)
            .collect();

        Ok(Proxy {
            pool: self.pool,
            providers: RwLock::new(providers),
            default_timeout: self.config.default_timeout,
            client,
        })
    }
}

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    providers: RwLock<Vec<Arc<dyn ChatModelProvider>>>,
    default_timeout: Option<Duration>,
    client: reqwest::Client,
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

        let send_start = tokio::time::Instant::now();
        let response = provider
            .send_request(SendRequestOptions {
                retry_options: options.retry,
                timeout: options
                    .timeout
                    .or(self.default_timeout)
                    .unwrap_or(Duration::from_secs(60)),
                api_key: options.api_key,
                body,
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
            }
            Err(e) => {
                todo!()
            }
        }

        // self.log_tx.send(log_entry).await.ok();

        response.map(|r| r.body)
    }

    pub async fn shutdown(&mut self) {
        // TODO if the database logger is active, close it down.
        // let log_tx = self.log_tx.take();
        // drop(log_tx)
        // let log_task = self.log_task.take();
        // log_task.await
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
