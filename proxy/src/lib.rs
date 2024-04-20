use std::{collections::BTreeMap, fmt::Debug, path::PathBuf, sync::Arc};

pub mod database;
pub mod error;
pub mod format;
pub mod providers;
pub mod request;

use database::Pool;
pub use error::Error;
use error_stack::Report;
use format::{ChatRequest, ChatResponse};
use providers::ChatModelProvider;
use request::RetryOptions;
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub type AnyChatModelProvider = Arc<dyn ChatModelProvider>;

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    config_path: Option<PathBuf>,
    providers: Vec<Arc<dyn ChatModelProvider>>,
    default_provider: Option<Arc<dyn ChatModelProvider>>,
    client: reqwest::Client,
}

impl Proxy {
    pub async fn new(
        database_pool: Option<Pool>,
        config_path: Option<PathBuf>,
    ) -> Result<Self, Error> {
        // todo load the providers from the database and from the config file if present

        // TODO make a builder interface for this
        Ok(Self {
            pool: database_pool,
            config_path,
            default_provider: None,
            providers: vec![],
            // todo allow passing an existing client, or maybe options for one? We still
            client: reqwest::Client::new(),
        })
    }

    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
    }

    pub fn default_provider_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.is_default_for_model(model))
            .map(Arc::clone)
            .or_else(|| self.default_provider.clone())
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
        let response = provider.send_request(options.retry, body).await;
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
    pub timeout: std::time::Duration,
    pub retry: RetryOptions,
    pub metadata: ProxyRequestMetadata,
}

impl Default for ProxyRequestOptions {
    fn default() -> Self {
        Self {
            model: None,
            provider: None,
            retry: RetryOptions::default(),
            timeout: std::time::Duration::from_secs(60),
            metadata: ProxyRequestMetadata::default(),
        }
    }
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
    /// The organization_id of the user that triggered the request
    pub organization_id: Option<String>,
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
