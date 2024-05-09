use std::{borrow::Cow, fmt::Debug, str::FromStr, sync::Arc, time::Duration};

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
use chrono::Utc;
use config::{AliasConfig, ApiKeyConfig};
use database::logging::ProxyLogEntry;
pub use error::Error;
use error_stack::{Report, ResultExt};
use format::{ChatRequest, ChatResponse};
use http::HeaderMap;
use provider_lookup::ProviderLookup;
use providers::ChatModelProvider;
use request::RetryOptions;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use tracing::instrument;
use uuid::Uuid;

use crate::request::try_model_choices;

pub type AnyChatModelProvider = Arc<dyn ChatModelProvider>;

#[derive(Debug, Serialize)]
pub struct ProxiedChatResponseMeta {
    /// A UUID assigned by Chronicle to the request, which is linked to the logged information.
    /// This is different from the `id` returned at the top level of the [ChatResponse], which
    /// comes from the provider itself.
    pub id: Uuid,
    /// Which provider was used for the request.
    pub provider: String,
    pub response_meta: Option<serde_json::Value>,
    pub was_rate_limited: bool,
}

#[derive(Debug, Serialize)]
pub struct ProxiedChatResponse {
    #[serde(flatten)]
    pub response: ChatResponse,
    pub meta: ProxiedChatResponseMeta,
}

#[derive(Deserialize, Debug)]
pub struct EventPayload {
    #[serde(rename = "type")]
    pub typ: String,
    pub data: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub metadata: ProxyRequestMetadata,
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

    /// Record an event to the database. This lets you have your LLM request events and other
    /// events in the same database table.
    pub async fn record_event(
        &self,
        internal_metadata: ProxyRequestInternalMetadata,
        mut body: EventPayload,
    ) -> Uuid {
        let id = Uuid::now_v7();

        let Some(log_tx) = &self.log_tx else {
            return id;
        };

        if body
            .metadata
            .extra
            .as_ref()
            .map(|m| m.is_empty())
            .unwrap_or(true)
        {
            // The logger gets the `meta` field from here, so just move it over.
            body.metadata.extra = match body.data {
                Some(serde_json::Value::Object(m)) => Some(m),
                _ => None,
            };
        }

        let log_entry = ProxyLogEntry {
            id,
            event_type: Cow::Owned(body.typ),
            timestamp: Utc::now(),
            request: None,
            response: None,
            total_latency: None,
            was_rate_limited: None,
            num_retries: None,
            error: body.error.and_then(|e| serde_json::to_string(&e).ok()),
            options: ProxyRequestOptions {
                metadata: body.metadata,
                internal_metadata,
                ..Default::default()
            },
        };

        log_tx.send_async(log_entry).await.ok();

        id
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
                    .filter_map(|m| {
                        let Some(content) = m.content.as_deref() else {
                            return None;
                        };

                        Some(format!(
                            "{}: {}",
                            m.name.as_ref().unwrap_or(&m.role),
                            content
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            ))
        } else {
            body.messages
                .get(0)
                .and_then(|m| m.content.as_deref().map(Cow::Borrowed))
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

        let retry = options.retry.clone().unwrap_or_default();

        let timestamp = chrono::Utc::now();
        let send_start = tokio::time::Instant::now();
        let response = try_model_choices(
            models,
            options.override_url.clone(),
            retry,
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
                current_span.record(
                    "llm.completions",
                    response
                        .body
                        .choices
                        .iter()
                        .filter_map(|c| c.message.content.as_deref())
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
                        event_type: Cow::Borrowed("chronicle_llm_request"),
                        timestamp,
                        request: Some(body.clone()),
                        response: Some(response.clone()),
                        num_retries: Some(response.num_retries),
                        was_rate_limited: Some(response.was_rate_limited),
                        total_latency: Some(send_start.elapsed()),
                        error: None,
                        options,
                    };

                    log_tx.send_async(log_entry).await.ok();
                }
            }
            Err(e) => {
                tracing::error!(error.full=?e.error, "Request failed");

                current_span.record("error", e.error.to_string());
                current_span.record("llm.retries", e.num_retries);
                current_span.record("llm.rate_limited", e.was_rate_limited);

                if let Some(log_tx) = &self.log_tx {
                    let log_entry = ProxyLogEntry {
                        id,
                        event_type: Cow::Borrowed("chronicle_llm_request"),
                        timestamp,
                        request: Some(body),
                        response: None,
                        total_latency: Some(send_start.elapsed()),
                        num_retries: Some(e.num_retries),
                        was_rate_limited: Some(e.was_rate_limited),
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
                    provider: r.provider,
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

#[serde_as]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProxyRequestOptions {
    /// Override the model from the request body or select an alias.
    /// This can also be set by passing the x-chronicle-model HTTP header.
    pub model: Option<String>,
    /// Choose a specific provider to use. This can also be set by passing the
    /// x-chronicle-provider HTTP header.
    pub provider: Option<String>,
    /// Force the provider to use a specific URL instead of its default. This can also be set
    /// by passing the x-chronicle-override-url HTTP header.
    pub override_url: Option<String>,
    /// An API key to pass to the provider. This can also be set by passing the
    /// x-chronicle-provider-api-key HTTP header.
    pub api_key: Option<String>,
    /// Supply multiple provider/model choices, which will be tried in order.
    /// If this is provided, the `model`, `provider`, and `api_key` fields are ignored.
    /// This field can not reference model aliases.
    /// This can also be set by passing the x-chronicle-models HTTP header using JSON syntax.
    #[serde(default)]
    pub models: Vec<ModelAndProvider>,
    /// When using `models` to supply multiple choices, start at a random choice instead of the
    /// first one.
    /// This can also be set by passing the x-chronicle-random-choice HTTP header.
    pub random_choice: Option<bool>,
    #[serde_as(as = "Option<DurationMilliSeconds>")]
    /// Customize the proxy's timeout when waiting for the request.
    /// This can also be set by passing the x-chronicle-timeout HTTP header.
    pub timeout: Option<std::time::Duration>,
    /// Customize the retry behavior. This can also be set by passing the
    /// x-chronicle-retry HTTP header.
    pub retry: Option<RetryOptions>,

    /// Metadata to record for the request
    #[serde(default)]
    pub metadata: ProxyRequestMetadata,

    /// Internal user authentication metadata for the request. This can be useful if you have a
    /// different set of internal users and organizations than what gets recorded in `metadata`.
    #[serde(skip, default)]
    pub internal_metadata: ProxyRequestInternalMetadata,
}

impl ProxyRequestOptions {
    pub fn merge_request_headers(&mut self, headers: &HeaderMap) -> Result<(), Report<Error>> {
        get_header_str(&mut self.api_key, headers, "x-chronicle-provider-api-key");
        get_header_str(&mut self.provider, headers, "x-chronicle-provider");
        get_header_str(&mut self.model, headers, "x-chronicle-model");
        get_header_str(&mut self.override_url, headers, "x-chronicle-override-url");

        let models_header = headers
            .get("x-chronicle-models")
            .map(|s| serde_json::from_slice::<Vec<ModelAndProvider>>(s.as_bytes()))
            .transpose()
            .change_context_lazy(|| Error::ReadingHeader("x-chronicle-models".to_string()))?;
        if let Some(models_header) = models_header {
            self.models = models_header;
        }

        get_header_t(
            &mut self.random_choice,
            headers,
            "x-chronicle-random-choice",
        )?;
        get_header_json(&mut self.retry, headers, "x-chronicle-retry")?;

        let timeout = headers
            .get("x-chronicle-timeout")
            .and_then(|s| s.to_str().ok())
            .map(|s| s.parse::<u64>())
            .transpose()
            .change_context_lazy(|| Error::ReadingHeader("x-chronicle-timeout".to_string()))?
            .map(|s| std::time::Duration::from_millis(s));
        if timeout.is_some() {
            self.timeout = timeout;
        }

        self.metadata.merge_request_headers(headers)?;

        Ok(())
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
    /// The application making this call. This can also be set by passing the
    /// x-chronicle-application HTTP header.
    pub application: Option<String>,
    /// The environment the application is running in. This can also be set by passing the
    /// x-chronicle-environment HTTP header.
    pub environment: Option<String>,
    /// The organization related to the request. This can also be set by passing the
    /// x-chronicle-organization-id HTTP header.
    pub organization_id: Option<String>,
    /// The project related to the request. This can also be set by passing the
    /// x-chronicle-project-id HTTP header.
    pub project_id: Option<String>,
    /// The id of the user that triggered the request. This can also be set by passing the
    /// x-chronicle-user-id HTTP header.
    pub user_id: Option<String>,
    /// The id of the workflow that this request belongs to. This can also be set by passing the
    /// x-chronicle-workflow-id HTTP header.
    pub workflow_id: Option<String>,
    /// A readable name of the workflow that this request belongs to. This can also be set by
    /// passing the x-chronicle-workflow-name HTTP header.
    pub workflow_name: Option<String>,
    /// The id of of the specific run that this request belongs to. This can also be set by
    /// passing the x-chronicle-run-id HTTP header.
    pub run_id: Option<String>,
    /// The name of the workflow step. This can also be set by passing the
    /// x-chronicle-step HTTP header.
    pub step: Option<String>,
    /// The index of the step within the workflow. This can also be set by passing the
    /// x-chronicle-step-index HTTP header.
    pub step_index: Option<u32>,
    /// A unique ID for this prompt. This can also be set by passing the
    /// x-chronicle-prompt-id HTTP header.
    pub prompt_id: Option<String>,
    /// The version of this prompt. This can also be set by passing the
    /// x-chronicle-prompt-version HTTP header.
    pub prompt_version: Option<u32>,

    /// Any other metadata to include. When passing this in the request body, any unknown fields
    /// are collected here. This can also be set by passing a JSON object to the
    /// x-chronicle-extra-meta HTTP header.
    #[serde(flatten)]
    pub extra: Option<serde_json::Map<String, serde_json::Value>>,
}

impl ProxyRequestMetadata {
    pub fn merge_request_headers(&mut self, headers: &HeaderMap) -> Result<(), Report<Error>> {
        get_header_str(&mut self.application, headers, "x-chronicle-application");
        get_header_str(&mut self.environment, headers, "x-chronicle-environment");
        get_header_str(
            &mut self.organization_id,
            headers,
            "x-chronicle-organization-id",
        );
        get_header_str(&mut self.project_id, headers, "x-chronicle-project-id");
        get_header_str(&mut self.user_id, headers, "x-chronicle-user-id");
        get_header_str(&mut self.workflow_id, headers, "x-chronicle-workflow-id");
        get_header_str(
            &mut self.workflow_name,
            headers,
            "x-chronicle-workflow-name",
        );
        get_header_str(&mut self.run_id, headers, "x-chronicle-run-id");
        get_header_str(&mut self.step, headers, "x-chronicle-step");
        get_header_t(&mut self.step_index, headers, "x-chronicle-step-index")?;
        get_header_str(&mut self.prompt_id, headers, "x-chronicle-prompt-id");
        get_header_t(
            &mut self.prompt_version,
            headers,
            "x-chronicle-prompt-version",
        )?;
        get_header_json(&mut self.extra, headers, "x-chronicle-extra-meta")?;
        Ok(())
    }
}

fn get_header_str(body_value: &mut Option<String>, headers: &HeaderMap, key: &str) {
    if body_value.is_some() {
        return;
    }

    let value = headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| s.to_string());

    if value.is_some() {
        *body_value = value;
    }
}

fn get_header_t<T>(
    body_value: &mut Option<T>,
    headers: &HeaderMap,
    key: &str,
) -> Result<(), Report<Error>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    if body_value.is_some() {
        return Ok(());
    }

    let value = headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| s.parse::<T>())
        .transpose()
        .change_context_lazy(|| Error::ReadingHeader(key.to_string()))?;

    if value.is_some() {
        *body_value = value;
    }

    Ok(())
}

fn get_header_json<T: DeserializeOwned>(
    body_value: &mut Option<T>,
    headers: &HeaderMap,
    key: &str,
) -> Result<(), Report<Error>> {
    if body_value.is_some() {
        return Ok(());
    }

    let value = headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| serde_json::from_str(s))
        .transpose()
        .change_context_lazy(|| Error::ReadingHeader(key.to_string()))?;

    if value.is_some() {
        *body_value = value;
    }

    Ok(())
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
                        content: Some("hello".to_string()),
                        tool_calls: Vec::new(),
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

        let mut result = proxy
            .send(
                crate::ProxyRequestOptions {
                    ..Default::default()
                },
                ChatRequest {
                    model: Some("me/a-test-model".to_string()),
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: Some("hello".to_string()),
                        tool_calls: Vec::new(),
                        name: None,
                    }],
                    ..Default::default()
                },
            )
            .await
            .expect("should have succeeded");

        // ID will be different every time, so zero it for the snapshot
        result.meta.id = uuid::Uuid::nil();
        insta::assert_json_snapshot!(result);
    }
}
