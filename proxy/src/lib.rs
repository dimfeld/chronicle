use std::{
    fmt::Debug,
    sync::{Arc, RwLock},
    time::Duration,
};

pub mod builder;
pub mod config;
pub mod database;
pub mod error;
pub mod format;
pub mod providers;
pub mod request;

use builder::ProxyBuilder;
use config::{AliasConfig, ApiKeyConfig};
use database::logging::ProxyLogEntry;
pub use error::Error;
use error_stack::Report;
use format::{ChatRequest, ChatResponse};
use providers::ChatModelProvider;
use request::RetryOptions;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::providers::SendRequestOptions;

pub type AnyChatModelProvider = Arc<dyn ChatModelProvider>;

#[derive(Debug)]
struct ProviderLookup {
    providers: Vec<Arc<dyn ChatModelProvider>>,
    aliases: Vec<AliasConfig>,
    api_keys: Vec<ApiKeyConfig>,
}

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    log_tx: Option<flume::Sender<ProxyLogEntry>>,
    log_task: Option<tokio::task::JoinHandle<()>>,
    lookup: RwLock<ProviderLookup>,
    default_timeout: Option<Duration>,
}

struct ModelLookupResult {
    from_options: bool,
    model: String,
    provider: Arc<dyn ChatModelProvider>,
    api_key: Option<String>,
}

impl Proxy {
    pub fn builder() -> ProxyBuilder {
        ProxyBuilder::new()
    }

    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.lookup
            .read()
            .unwrap()
            .providers
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
    }

    pub fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.lookup
            .read()
            .unwrap()
            .providers
            .iter()
            .find(|p| p.is_default_for_model(model))
            .map(Arc::clone)
    }

    fn find_model_and_provider<'a>(
        &self,
        options: &'a ProxyRequestOptions,
        body: &'a ChatRequest,
    ) -> Result<ModelLookupResult, Error> {
        let (from_options, model) = if let Some(model) = &options.model {
            (true, model.as_str())
        } else {
            (false, body.model.as_deref().unwrap_or_default())
        };

        if model.is_empty() {
            return Err(Error::ModelNotSpecified);
        }

        let lookup = self.lookup.read().unwrap();
        let alias = lookup.aliases.iter().find(|alias| alias.name == model);

        let provider = if let Some(alias) = alias {
            lookup
                .providers
                .iter()
                .find(|p| p.name() == alias.provider)
                .ok_or_else(|| Error::NoAliasProvider(alias.name.clone(), alias.provider.clone()))?
                .clone()
        } else if let Some(provider_name) = options.provider.as_deref() {
            self.get_provider(provider_name)
                .ok_or_else(|| Error::UnknownProvider(provider_name.to_string()))?
        } else {
            self.default_provider_for_model(model)
                .ok_or_else(|| Error::NoDefault(model.to_string()))?
        };

        let model = alias.map(|alias| alias.model.as_str()).unwrap_or(model);

        let api_key = alias
            .and_then(|alias| {
                if let Some(key_name) = alias.api_key_name.as_deref() {
                    let key = lookup
                        .api_keys
                        .iter()
                        .find(|key| key.name == key_name)
                        .map(|key| key.value.clone())
                        .ok_or_else(|| {
                            Error::NoAliasApiKey(alias.name.clone(), key_name.to_string())
                        });

                    Some(key)
                } else {
                    None
                }
            })
            .transpose()?;

        Ok(ModelLookupResult {
            from_options: from_options || alias.is_some(),
            provider,
            model: model.to_string(),
            api_key,
        })
    }

    /// Send a request, choosing the provider based on the requested `model` and `provider`.
    ///
    /// `options.provider` can be used to choose a specific provider.
    /// `options.model` will be used next to choose a model to use
    /// `body["model"]` is used if options.model is empty.
    #[instrument(fields(
        latency,
        total_latency,
        retries,
        rate_limited,
        tokens_input,
        tokens_output,
        status_code
    ))]
    pub async fn send(
        &self,
        options: ProxyRequestOptions,
        mut body: ChatRequest,
    ) -> Result<ChatResponse, Report<Error>> {
        let ModelLookupResult {
            from_options,
            provider,
            model,
            api_key,
        } = self.find_model_and_provider(&options, &body)?;

        if from_options {
            // Update the body to have the correct model we actually want, in case
            // the model was an alias or the options overrode it.
            body.model = Some(model.to_string());
        }

        let api_key = if options.api_key.is_some() {
            options.api_key.clone()
        } else {
            api_key
        };

        tracing::info!(?body, "Starting request");

        let timestamp = chrono::Utc::now();
        let send_start = tokio::time::Instant::now();
        let response = provider
            .send_request(SendRequestOptions {
                retry_options: options.retry.clone(),
                timeout: options
                    .timeout
                    .or(self.default_timeout)
                    .unwrap_or(Duration::from_secs(60)),
                api_key,
                body: body.clone(),
            })
            .await;
        let send_time = send_start.elapsed().as_millis();

        // Get response stats: latency, tokens used, etc.
        // We want to record both the total latency and the latency of the final working request
        // Hopefully we can do that in a way that allows Postgres to use a HOT update
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
