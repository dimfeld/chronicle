//! Utilities for retrying a provider request as needed.
use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use tracing::instrument;

use crate::{
    format::{
        ChatRequest, StreamingChatResponse, StreamingResponse, StreamingResponseInfo,
        SynchronousChatResponse,
    },
    provider_lookup::{ModelLookupChoice, ModelLookupResult},
    providers::{
        ProviderError, ProviderErrorKind, SendRequestOptions, SynchronousProviderResponse,
    },
    Error,
};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryOptions {
    /// How long to wait after the first failure.
    /// The default value is 200ms.
    #[serde_as(as = "DurationMilliSeconds")]
    #[serde(default = "default_initial_backoff")]
    initial_backoff: Duration,

    /// How to increase the backoff duration as additional retries occur. The default value
    /// is an exponential backoff with a multiplier of `2.0`.
    #[serde(default)]
    increase: RepeatBackoffBehavior,

    /// The number of times to try the request, including the first try.
    /// Defaults to 4.
    #[serde(default = "default_max_tries")]
    max_tries: u32,

    /// Maximum amount of jitter to add. The added jitter will be a random value between 0 and this
    /// value.
    /// Defaults to 100ms.
    #[serde_as(as = "DurationMilliSeconds")]
    #[serde(default = "default_jitter")]
    jitter: Duration,

    /// Never wait more than this amount of time. The behavior of this flag may be modified by the
    /// `fail_if_rate_limit_exceeds_max_backoff` flag.
    #[serde_as(as = "DurationMilliSeconds")]
    #[serde(default = "default_max_backoff")]
    max_backoff: Duration,

    /// If a rate limit response asks to wait longer than the `max_backoff`, then stop retrying.
    /// Otherwise it will wait for the requested time even if it is longer than max_backoff.
    /// Defaults to true.
    #[serde(default = "true_t")]
    fail_if_rate_limit_exceeds_max_backoff: bool,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            initial_backoff: default_initial_backoff(),
            increase: RepeatBackoffBehavior::default(),
            max_backoff: default_max_backoff(),
            max_tries: default_max_tries(),
            jitter: default_jitter(),
            fail_if_rate_limit_exceeds_max_backoff: true,
        }
    }
}

const fn default_max_tries() -> u32 {
    4
}

const fn default_initial_backoff() -> Duration {
    Duration::from_millis(200)
}

const fn default_jitter() -> Duration {
    Duration::from_millis(100)
}

const fn default_max_backoff() -> Duration {
    Duration::from_millis(5000)
}

const fn true_t() -> bool {
    true
}

/// How to increase the backoff duration as additional retries occur.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde_as]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RepeatBackoffBehavior {
    /// Use the initial backoff duration for additional retries as well.
    Constant,
    /// Add this duration to the backoff duration after each retry.
    Additive {
        #[serde_as(as = "DurationMilliSeconds")]
        amount: Duration,
    },
    /// Multiply the backoff duration by this value after each retry.
    Exponential { multiplier: f64 },
}

impl Default for RepeatBackoffBehavior {
    fn default() -> Self {
        Self::Exponential { multiplier: 2.0 }
    }
}

impl RepeatBackoffBehavior {
    fn next(&self, current: Duration) -> Duration {
        match self {
            RepeatBackoffBehavior::Constant => current,
            RepeatBackoffBehavior::Additive { amount } => {
                Duration::from_nanos(current.as_nanos() as u64 + amount.as_nanos() as u64)
            }
            RepeatBackoffBehavior::Exponential { multiplier } => {
                Duration::from_nanos((current.as_nanos() as f64 * multiplier) as u64)
            }
        }
    }
}

struct BackoffValue<'a> {
    next_backoff: Duration,
    options: &'a RetryOptions,
}

impl<'a> BackoffValue<'a> {
    fn new(options: &'a RetryOptions) -> Self {
        Self {
            next_backoff: options.initial_backoff,
            options,
        }
    }

    /// Return the next duration to wait for
    fn next(&mut self) -> Duration {
        let mut backoff = self.next_backoff;
        self.next_backoff = self.options.increase.next(backoff);

        let max_jitter = self.options.jitter.as_secs_f64();
        if max_jitter > 0.0 {
            let jitter_value = rand::thread_rng().gen_range::<f64, _>(0.0..=1.0) * max_jitter;
            backoff += Duration::from_secs_f64(jitter_value);
        }

        backoff.min(self.options.max_backoff)
    }
}

#[derive(Debug, Clone)]
pub enum ProxiedResultBody {
    Streaming(flume::Receiver<StreamingResponse>),
    Synchronous(SynchronousProviderResponse),
}

#[derive(Debug, Clone)]
pub struct ProxiedResult {
    pub body: ProxiedResultBody,
    /// The provider which was used for the successful response.
    pub provider: String,
    pub num_retries: u32,
    pub was_rate_limited: bool,
}

#[derive(Debug)]
pub struct ProxiedResultError {
    pub error: Report<Error>,
    pub num_retries: u32,
    pub was_rate_limited: bool,
}

/// Run a provider request and retry on failure.
#[instrument(level = "debug")]
pub async fn try_model_choices(
    ModelLookupResult {
        alias,
        random_order,
        choices,
    }: ModelLookupResult,
    override_url: Option<String>,
    options: RetryOptions,
    timeout: Duration,
    request: ChatRequest,
) -> Result<ProxiedResult, ProxiedResultError> {
    let single_choice = choices.len() == 1;
    let start_choice = if random_order && !single_choice {
        rand::thread_rng().gen_range(0..choices.len())
    } else {
        0
    };

    let mut current_choice = start_choice;

    let mut on_final_model_choice = single_choice;
    let mut backoff = BackoffValue::new(&options);

    let mut was_rate_limited = false;
    let mut current_try: u32 = 1;

    loop {
        let ModelLookupChoice {
            model,
            provider,
            api_key,
        } = &choices[current_choice];

        let mut body = request.clone();
        body.model = Some(model.to_string());
        let result = provider
            .send_request(SendRequestOptions {
                override_url: override_url.clone(),
                timeout,
                api_key: api_key.clone(),
                body,
            })
            .await;

        let provider_name = provider.name();
        let error = match result {
            Ok(value) => {
                return Ok(ProxiedResult {
                    body: ProxiedResultBody::Synchronous(value),
                    provider: provider_name.to_string(),
                    num_retries: current_try - 1,
                    was_rate_limited,
                });
            }
            Err(e) => {
                tracing::error!(err=?e, "llm.try"=current_try - 1, llm.vendor=provider_name, llm.request.model = model, llm.alias=alias);
                e.attach_printable(format!(
                    "Try {current_try}, Provider: {provider_name}, Model: {model}"
                ))
            }
        };

        let provider_error = error
            .frames()
            .find_map(|frame| frame.downcast_ref::<ProviderError>());

        if let Some(ProviderErrorKind::RateLimit { .. }) = provider_error.map(|e| &e.kind) {
            was_rate_limited = true;
        }

        // If we don't have any more fallback models, and this error is not retryable, then exit.
        if current_try == options.max_tries
            || (on_final_model_choice
                && !provider_error.map(|e| e.kind.retryable()).unwrap_or(false))
        {
            return Err(ProxiedResultError {
                error,
                num_retries: current_try - 1,
                was_rate_limited,
            });
        }

        if !on_final_model_choice {
            if current_choice == choices.len() - 1 {
                current_choice = 0;
            } else {
                current_choice = current_choice + 1;
            }

            if current_choice == start_choice {
                // We looped around to the first choice again so enable backoff if it wasn't already
                // on.
                on_final_model_choice = true;
            }
        }

        if on_final_model_choice {
            // If we're on the final model choice then we need to backoff before the next retry.
            let wait = backoff.next();
            let wait = match provider_error.map(|e| &e.kind) {
                // Rate limited, where the provider specified a time to wait
                Some(ProviderErrorKind::RateLimit {
                    retry_after: Some(retry_after),
                }) => {
                    if options.fail_if_rate_limit_exceeds_max_backoff
                        && *retry_after > options.max_backoff
                    {
                        // Rate limited with a retry time that exceeds max backoff.
                        return Err(ProxiedResultError {
                            error,
                            num_retries: current_try - 1,
                            was_rate_limited,
                        });
                    }

                    // If the rate limit retry duration is more than the planned wait, then wait for
                    // the rate limit duration instead.
                    wait.max(*retry_after)
                }
                _ => wait,
            };

            tokio::time::sleep(wait).await;
        }

        current_try += 1;
    }
}

/// Send an HTTP request with retries, and handle errors.
/// Most providers can use this to handle sending their request and handling errors.
#[instrument(level = "debug", skip(body, prepare, handle_rate_limit))]
pub async fn send_standard_request(
    timeout: Duration,
    prepare: impl Fn() -> reqwest::RequestBuilder,
    handle_rate_limit: impl Fn(&reqwest::Response) -> Option<Duration>,
    body: Bytes,
) -> Result<(reqwest::Response, Duration), Report<ProviderError>> {
    let start = tokio::time::Instant::now();
    let result = prepare()
        .timeout(timeout)
        .body(body)
        .send()
        .await
        .change_context(ProviderError {
            kind: ProviderErrorKind::Sending,
            status_code: None,
            body: None,
            latency: start.elapsed(),
        })?;

    let status = result.status();
    let error = ProviderErrorKind::from_status_code(status);

    if let Some(mut e) = error {
        match &mut e {
            ProviderErrorKind::RateLimit { retry_after } => {
                let value = handle_rate_limit(&result);
                *retry_after = value;
            }
            _ => {}
        };

        let body = result.json::<serde_json::Value>().await.ok();
        let latency = start.elapsed();

        Err(Report::new(ProviderError {
            kind: e,
            status_code: Some(status),
            body,
            latency,
        }))
    } else {
        let latency = start.elapsed();
        Ok::<_, Report<ProviderError>>((result, latency))
    }
}

/// Parse a JSON response, with informative errors when the format does not match the expected
/// structure.
pub async fn parse_response_json<RESPONSE: DeserializeOwned>(
    response: reqwest::Response,
    latency: Duration,
) -> Result<RESPONSE, Report<ProviderError>> {
    let status = response.status();

    // Get the result as text first so that we can save the entire response for better
    // introspection if parsing fails.
    let text = response.text().await.change_context(ProviderError {
        kind: ProviderErrorKind::ParsingResponse,
        status_code: Some(status),
        body: None,
        latency,
    })?;

    let jd = &mut serde_json::Deserializer::from_str(&text);
    let body: RESPONSE = serde_path_to_error::deserialize(jd).change_context(ProviderError {
        kind: ProviderErrorKind::ParsingResponse,
        status_code: Some(status),
        body: Some(serde_json::Value::String(text)),
        latency,
    })?;

    Ok(body)
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::ProxiedResultError;
    use crate::{
        format::{ChatMessage, ChatRequest},
        provider_lookup::{ModelLookupChoice, ModelLookupResult},
        request::{try_model_choices, ProxiedResult, RetryOptions},
    };

    async fn test_request(
        choices: Vec<ModelLookupChoice>,
    ) -> Result<ProxiedResult, ProxiedResultError> {
        try_model_choices(
            ModelLookupResult {
                alias: String::new(),
                random_order: false,
                choices,
            },
            None,
            RetryOptions::default(),
            Duration::from_secs(5),
            ChatRequest {
                messages: vec![ChatMessage {
                    role: Some("user".to_string()),
                    content: Some("Tell me a story".to_string()),
                    tool_calls: Vec::new(),
                    name: None,
                }],
                ..Default::default()
            },
        )
        .await
    }

    mod single_choice {
        use std::sync::Arc;

        use super::test_request;
        use crate::{provider_lookup::ModelLookupChoice, testing::TestProvider};

        #[tokio::test(start_paused = true)]
        async fn success() {
            let response = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: TestProvider::default().into(),
                api_key: None,
            }])
            .await
            .expect("Failed");

            assert_eq!(response.num_retries, 0);
            assert_eq!(response.was_rate_limited, false);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn nonretryable_failures() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::BadRequest),
                ..Default::default()
            });
            let response = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: provider.clone(),
                api_key: None,
            }])
            .await
            .expect_err("Should have failed");

            assert_eq!(provider.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
            assert_eq!(response.num_retries, 0);
            assert_eq!(response.was_rate_limited, false);
        }

        #[tokio::test(start_paused = true)]
        async fn transient_failure() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::Transient),
                fail_times: 2,
                ..Default::default()
            });
            let response = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: provider.clone(),
                api_key: None,
            }])
            .await
            .expect("Should succeed");

            assert_eq!(
                provider.calls.load(std::sync::atomic::Ordering::Relaxed),
                3,
                "Should succeed on third try"
            );
            assert_eq!(response.num_retries, 2);
            assert_eq!(response.was_rate_limited, false);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn rate_limit() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::RateLimit),
                fail_times: 2,
                ..Default::default()
            });
            let response = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: provider.clone(),
                api_key: None,
            }])
            .await
            .expect("Should succeed");

            assert_eq!(
                provider.calls.load(std::sync::atomic::Ordering::Relaxed),
                3,
                "Should succeed on third try"
            );
            assert_eq!(response.num_retries, 2);
            assert_eq!(response.was_rate_limited, true);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn max_retries() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::Transient),
                ..Default::default()
            });
            let response = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: provider.clone(),
                api_key: None,
            }])
            .await
            .expect_err("Should have failed");

            assert_eq!(
                provider.calls.load(std::sync::atomic::Ordering::Relaxed),
                4,
                "Should have tried 4 times"
            );
            assert_eq!(response.num_retries, 3);
            assert_eq!(response.was_rate_limited, false);
        }
    }

    mod multiple_choices {
        use std::sync::Arc;

        use super::test_request;
        use crate::{
            provider_lookup::ModelLookupChoice,
            testing::{TestFailure, TestProvider},
        };

        #[tokio::test(start_paused = true)]
        async fn success() {
            let response = test_request(vec![
                ModelLookupChoice {
                    model: "test-model".to_string(),
                    provider: TestProvider::default().into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-2".to_string(),
                    provider: TestProvider::default().into(),
                    api_key: None,
                },
            ])
            .await
            .expect("Failed");

            assert_eq!(response.num_retries, 0);
            assert_eq!(response.was_rate_limited, false);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn transient_failures() {
            let response = test_request(vec![
                ModelLookupChoice {
                    model: "test-model".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::Transient),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-2".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::Transient),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-3".to_string(),
                    provider: TestProvider::default().into(),
                    api_key: None,
                },
            ])
            .await
            .expect("Failed");

            assert_eq!(response.num_retries, 2);
            assert_eq!(response.was_rate_limited, false);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model-3");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn rate_limit() {
            let response = test_request(vec![
                ModelLookupChoice {
                    model: "test-model".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::RateLimit),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-2".to_string(),
                    provider: TestProvider::default().into(),
                    api_key: None,
                },
            ])
            .await
            .expect("Failed");

            assert_eq!(response.num_retries, 1);
            assert_eq!(response.was_rate_limited, true);
            assert_eq!(response.provider, "test");
            assert_eq!(response.body.model.unwrap(), "test-model-2");
            assert_eq!(
                response.body.choices[0].message.content.as_deref().unwrap(),
                "A response"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn all_failed_every_time() {
            let response = test_request(vec![
                ModelLookupChoice {
                    model: "test-model".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::BadRequest),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-2".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::RateLimit),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-3".to_string(),
                    provider: TestProvider {
                        fail: Some(TestFailure::Transient),
                        ..Default::default()
                    }
                    .into(),
                    api_key: None,
                },
            ])
            .await
            .expect_err("Should have failed");

            assert_eq!(response.num_retries, 3);
            assert_eq!(response.was_rate_limited, true);
        }

        #[tokio::test(start_paused = true)]
        async fn all_failed_once() {
            let p1 = Arc::new(TestProvider {
                fail: Some(TestFailure::BadRequest),
                fail_times: 1,
                ..Default::default()
            });
            let p2 = Arc::new(TestProvider {
                fail: Some(TestFailure::RateLimit),
                ..Default::default()
            });
            let p3 = Arc::new(TestProvider {
                fail: Some(TestFailure::Transient),
                ..Default::default()
            });

            let response = test_request(vec![
                ModelLookupChoice {
                    model: "test-model".to_string(),
                    provider: p1.clone(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-2".to_string(),
                    provider: p2.clone(),
                    api_key: None,
                },
                ModelLookupChoice {
                    model: "test-model-3".to_string(),
                    provider: p3.clone(),
                    api_key: None,
                },
            ])
            .await
            .expect("Should have succeeded");

            assert_eq!(response.num_retries, 3);
            assert_eq!(response.was_rate_limited, true);
            assert_eq!(response.provider, "test");
            // Should have wrapped around to the first one again.
            assert_eq!(response.body.model.unwrap(), "test-model");
            assert_eq!(p1.calls.load(std::sync::atomic::Ordering::Relaxed), 2);
            assert_eq!(p2.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
            assert_eq!(p3.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        }
    }
}
