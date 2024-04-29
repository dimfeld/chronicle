use std::{borrow::Cow, fmt::Debug, sync::Arc, time::Duration};

pub mod builder;
pub mod config;
pub mod database;
pub mod error;
pub mod format;
mod provider_lookup;
pub mod providers;
pub mod request;
#[cfg(test)]
mod testing;

use builder::ProxyBuilder;
use config::{AliasConfig, ApiKeyConfig};
use database::logging::ProxyLogEntry;
pub use error::Error;
use error_stack::Report;
use format::{ChatRequest, ChatResponse};
use provider_lookup::ProviderLookup;
use providers::ChatModelProvider;
use request::RetryOptions;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::request::try_model_choices;

pub type AnyChatModelProvider = Arc<dyn ChatModelProvider>;

#[derive(Debug, Serialize)]
pub struct ProxiedChatResponseMeta {
    pub id: Uuid,
    pub response_meta: Option<serde_json::Value>,
    pub was_rate_limited: bool,
}

#[derive(Debug, Serialize)]
pub struct ProxiedChatResponse {
    #[serde(flatten)]
    pub response: ChatResponse,
    pub meta: ProxiedChatResponseMeta,
}

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    log_tx: Option<flume::Sender<ProxyLogEntry>>,
    log_task: Option<tokio::task::JoinHandle<()>>,
    lookup: ProviderLookup,
    client: reqwest::Client,
    default_timeout: Option<Duration>,
}

impl Proxy {
    pub fn builder() -> ProxyBuilder {
        ProxyBuilder::new()
    }

    /// Send a request, choosing the provider based on the requested `model` and `provider`.
    ///
    /// `options.models` can be used to specify a list of models and providers to use.
    /// `options.model` will be used next to choose a model to use. This and body["model"] can be
    /// an alias name.
    /// `options.provider` can be used to choose a specific provider if the model is not an alias.
    /// `body["model"]` is used if options.model is empty.
    #[instrument(
        name = "llm.send_request",
        skip(self, options),
        fields(
            error,
            llm.options=serde_json::to_string(&options).ok(),
            llm.item_id,
            llm.finish_reason,
            llm.latency,
            llm.total_latency,
            llm.retries,
            llm.rate_limited,
            llm.status_code,
            llm.meta.application = options.metadata.application,
            llm.meta.environment = options.metadata.environment,
            llm.meta.organization_id = options.metadata.organization_id,
            llm.meta.project_id = options.metadata.project_id,
            llm.meta.user_id = options.metadata.user_id,
            llm.meta.workflow_id = options.metadata.workflow_id,
            llm.meta.workflow_name = options.metadata.workflow_name,
            llm.meta.run_id = options.metadata.run_id,
            llm.meta.step = options.metadata.step,
            llm.meta.step_index = options.metadata.step_index,
            llm.meta.prompt_id = options.metadata.prompt_id,
            llm.meta.prompt_version = options.metadata.prompt_version,
            llm.meta.extra,
            llm.meta.internal_organization_id = options.internal_metadata.organization_id,
            llm.meta.internal_project_id = options.internal_metadata.project_id,
            llm.meta.internal_user_id = options.internal_metadata.user_id,
            // The fields below are using the OpenLLMetry field names
            llm.vendor,
            // This will work once https://github.com/tokio-rs/tracing/pull/2925 is merged
            // llm.request.type = "chat",
            llm.request.model = body.model,
            llm.prompts,
            llm.prompts.raw = serde_json::to_string(&body.messages).ok(),
            llm.request.max_tokens = body.max_tokens,
            llm.response.model,
            llm.usage.prompt_tokens,
            llm.usage.completion_tokens,
            llm.usage.total_tokens,
            llm.completions,
            llm.completions.raw,
            llm.temperature = body.temperature,
            llm.top_p = body.top_p,
            llm.frequency_penalty = body.frequency_penalty,
            llm.presence_penalty = body.presence_penalty,
            llm.chat.stop_sequences,
            llm.user = body.user,
        )
    )]
    pub async fn send(
        &self,
        options: ProxyRequestOptions,
        body: ChatRequest,
    ) -> Result<ProxiedChatResponse, Report<Error>> {
        let id = uuid::Uuid::now_v7();
        let current_span = tracing::Span::current();
        current_span.record("llm.item_id", id.to_string());
        if !body.stop.is_empty() {
            current_span.record(
                "llm.chat.stop_sequences",
                serde_json::to_string(&body.stop).ok(),
            );
        }

        if let Some(extra) = options.metadata.extra.as_ref().filter(|e| !e.is_empty()) {
            current_span.record("llm.meta.extra", &serde_json::to_string(extra).ok());
        }

        let messages_field = if body.messages.len() > 1 {
            Some(Cow::Owned(
                body.messages
                    .iter()
                    .map(|m| format!("{}: {}", m.name.as_ref().unwrap_or(&m.role), m.content))
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            ))
        } else {
            body.messages
                .get(0)
                .map(|m| Cow::Borrowed(m.content.as_str()))
        };
        current_span.record("llm.prompts", messages_field.as_deref());

        let models = self.lookup.find_model_and_provider(&options, &body)?;

        if models.choices.is_empty() {
            return Err(Report::new(Error::AliasEmpty(models.alias)));
        }

        if models.choices.len() == 1 {
            // If there's just one provider we can record this in advance to get it even in case of
            // error.
            current_span.record("llm.vendor", models.choices[0].provider.name());
        }

        tracing::info!(?body, "Starting request");

        let timestamp = chrono::Utc::now();
        let send_start = tokio::time::Instant::now();
        let response = try_model_choices(
            models,
            options.retry.clone(),
            options
                .timeout
                .or(self.default_timeout)
                .unwrap_or_else(|| Duration::from_millis(60_000)),
            body.clone(),
        )
        .await;

        let send_time = send_start.elapsed().as_millis();

        // In case of retries, this might be meaningfully different from the main latency.
        current_span.record("llm.total_latency", send_time);

        match &response {
            Ok(response) => {
                current_span.record("error", false);
                current_span.record(
                    "llm.completions",
                    response
                        .body
                        .choices
                        .iter()
                        .map(|c| c.message.content.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n"),
                );
                current_span.record(
                    "llm.completions.raw",
                    serde_json::to_string(&response.body.choices).ok(),
                );
                current_span.record("llm.vendor", &response.provider);
                current_span.record("llm.response.model", &response.body.model);
                current_span.record("llm.latency", response.latency.as_millis());
                current_span.record("llm.retries", response.num_retries);
                current_span.record("llm.rate_limited", response.was_rate_limited);
                current_span.record("llm.usage.prompt_tokens", response.body.usage.prompt_tokens);
                current_span.record(
                    "llm.finish_reason",
                    response.body.choices.get(0).map(|c| &c.finish_reason),
                );
                current_span.record(
                    "llm.usage.completion_tokens",
                    response.body.usage.completion_tokens,
                );
                let total_tokens = response.body.usage.total_tokens.unwrap_or_else(|| {
                    response.body.usage.prompt_tokens.unwrap_or(0)
                        + response.body.usage.completion_tokens.unwrap_or(0)
                });
                current_span.record("llm.usage.total_tokens", total_tokens);

                if let Some(log_tx) = &self.log_tx {
                    let log_entry = ProxyLogEntry {
                        id,
                        timestamp,
                        request: body.clone(),
                        response: Some(response.clone()),
                        num_retries: response.num_retries,
                        was_rate_limited: response.was_rate_limited,
                        total_latency: send_start.elapsed(),
                        error: None,
                        options,
                    };

                    log_tx.send_async(log_entry).await.ok();
                }
            }
            Err(e) => {
                tracing::error!(?e, "Request failed");

                current_span.record("error", true);
                current_span.record("llm.retries", e.num_retries);
                current_span.record("llm.rate_limited", e.was_rate_limited);

                if let Some(log_tx) = &self.log_tx {
                    let log_entry = ProxyLogEntry {
                        id,
                        timestamp,
                        request: body,
                        response: None,
                        total_latency: send_start.elapsed(),
                        num_retries: e.num_retries,
                        was_rate_limited: e.was_rate_limited,
                        error: Some(format!("{:?}", e)),
                        options,
                    };

                    log_tx.send_async(log_entry).await.ok();
                }
            }
        }

        response
            .map(|r| ProxiedChatResponse {
                response: r.body,
                meta: ProxiedChatResponseMeta {
                    id,
                    response_meta: r.meta,
                    was_rate_limited: r.was_rate_limited,
                },
            })
            .map_err(|e| e.error)
    }

    /// Add a provider to the system. This will replace any existing provider with the same `name`.
    pub fn set_provider(&self, provider: Arc<dyn ChatModelProvider>) {
        self.lookup.set_provider(provider);
    }

    /// Remove a provider. Any aliases that reference this provider's name will stop working.
    pub fn remove_provider(&self, name: &str) {
        self.lookup.remove_provider(name);
    }

    /// Add an alias to the system. This will replace any existing alias with the same `name`.
    pub fn set_alias(&self, alias: AliasConfig) {
        self.lookup.set_alias(alias);
    }

    /// Remove an alias
    pub fn remove_alias(&self, name: &str) {
        self.lookup.remove_alias(name);
    }

    /// Add an API key to the system. This will replace any existing API key with the same `name`.
    pub fn set_api_key(&self, api_key: ApiKeyConfig) {
        self.lookup.set_api_key(api_key);
    }

    /// Remove an API key. Any aliases that reference this API key's name will stop working.
    pub fn remove_api_key(&self, name: &str) {
        self.lookup.remove_api_key(name);
    }

    /// Shutdown the proxy, making sure to write any queued logging events
    pub async fn shutdown(&mut self) {
        let log_tx = self.log_tx.take();
        drop(log_tx);
        let log_task = self.log_task.take();
        if let Some(log_task) = log_task {
            log_task.await.ok();
        }
    }

    /// Validate the loaded configuration, and return a list of problems found.
    // todo this doesn't do anything yet
    fn validate(&self) -> Vec<String> {
        self.lookup.validate()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelAndProvider {
    pub model: String,
    pub provider: String,
    /// Supply an API key.
    pub api_key: Option<String>,
    /// Get the API key from a preconfigured key
    pub api_key_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyRequestOptions {
    /// Override the model from the request body or select an alias.
    pub model: Option<String>,
    /// Override the provider from the request body
    pub provider: Option<String>,
    /// An API key to use
    pub api_key: Option<String>,
    /// Supply multiple provider/model choices, which will be tried in order.
    /// If this is provided, the `model`, `provider`, and `api_key` fields are ignored.
    /// This field can not reference model aliases.
    #[serde(default)]
    pub models: Vec<ModelAndProvider>,
    /// When using `models` to supply multiple choices, start at a random choice instead of the
    /// first one.
    #[serde(default)]
    pub random_choice: bool,
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
            models: Vec::new(),
            random_choice: false,
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
    /// A unique ID for this prompt
    pub prompt_id: Option<String>,
    /// The version of this prompt
    pub prompt_version: Option<u32>,

    /// Any other metadata to include.
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };

    use crate::{
        config::CustomProviderConfig,
        format::{ChatChoice, ChatMessage, ChatRequest, ChatResponse, UsageResponse},
        providers::custom::{OpenAiRequestFormatOptions, ProviderRequestFormat},
        ProxyRequestMetadata,
    };

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

    #[tokio::test]
    async fn call_provider() {
        let mock_server = wiremock::MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ChatResponse {
                created: 1,
                model: None,
                system_fingerprint: None,
                usage: UsageResponse {
                    prompt_tokens: Some(1),
                    completion_tokens: Some(1),
                    total_tokens: Some(2),
                },
                choices: vec![ChatChoice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: "hello".to_string(),
                        name: None,
                    },
                    finish_reason: "stop".to_string(),
                }],
            }))
            .mount(&mock_server)
            .await;

        let url = format!("{}/v1/chat/completions", mock_server.uri());

        let proxy = super::Proxy::builder()
            .with_custom_provider(CustomProviderConfig {
                name: "test".to_string(),
                url,
                format: ProviderRequestFormat::OpenAi(OpenAiRequestFormatOptions {
                    transforms: crate::format::ChatRequestTransformation {
                        supports_message_name: false,
                        system_in_messages: true,
                        strip_model_prefix: Some("me/".into()),
                    },
                }),
                label: None,
                api_key: None,
                api_key_source: None,
                headers: BTreeMap::default(),
                prefix: Some("me/".to_string()),
            })
            .build()
            .await
            .expect("Building proxy");

        let result = proxy
            .send(
                crate::ProxyRequestOptions {
                    ..Default::default()
                },
                ChatRequest {
                    model: Some("me/a-test-model".to_string()),
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: "hello".to_string(),
                        name: None,
                    }],
                    ..Default::default()
                },
            )
            .await
            .expect("should have succeeded");

        insta::assert_json_snapshot!(result);
    }
}
