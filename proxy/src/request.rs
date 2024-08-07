//! Utilities for retrying a provider request as needed.
use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::{serde_as, DurationMilliSeconds};
use tracing::instrument;

use crate::{
    format::{ChatRequest, StreamingResponseSender},
    provider_lookup::{ModelLookupChoice, ModelLookupResult},
    providers::{ProviderError, ProviderErrorKind, SendRequestOptions},
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
pub struct TryModelChoicesResult {
    /// The provider which was used for the successful request.
    pub provider: String,
    /// The model which was used for the successful request
    pub model: String,
    /// How many times we had to retry before we got a successful response.
    pub num_retries: u32,
    /// If we retried due to hitting a rate limit.
    pub was_rate_limited: bool,
    /// When the latest, successful request started
    pub start_time: tokio::time::Instant,
}

#[derive(Debug)]
pub struct TryModelChoicesError {
    pub error: Report<ProviderError>,
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
    chunk_tx: StreamingResponseSender,
) -> Result<TryModelChoicesResult, TryModelChoicesError> {
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
        let start_time = tokio::time::Instant::now();
        let result = provider
            .send_request(
                SendRequestOptions {
                    override_url: override_url.clone(),
                    timeout,
                    api_key: api_key.clone(),
                    body,
                },
                chunk_tx.clone(),
            )
            .await;

        let provider_name = provider.name();
        let error = match result {
            Ok(_) => {
                // The caller will stream the response from here.
                return Ok(TryModelChoicesResult {
                    was_rate_limited,
                    num_retries: current_try - 1,
                    provider: provider.name().to_string(),
                    model: model.to_string(),
                    start_time,
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
            return Err(TryModelChoicesError {
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
                        return Err(TryModelChoicesError {
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
        .map_err(|e| {
            let kind = ProviderErrorKind::from_reqwest_send_error(&e);
            Report::new(e).change_context(ProviderError {
                kind,
                status_code: None,
                body: None,
                latency: start.elapsed(),
            })
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

        let body_text = result.text().await.ok();

        let body_json = body_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
        let latency = start.elapsed();

        Err(Report::new(ProviderError {
            kind: e,
            status_code: Some(status),
            body: body_json.or_else(|| body_text.map(serde_json::Value::String)),
            latency,
        }))
    } else {
        let latency = start.elapsed();
        Ok::<_, Report<ProviderError>>((result, latency))
    }
}

pub fn response_is_sse(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .map(|ct| ct.starts_with("text/event-stream"))
        .unwrap_or_default()
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

    use super::TryModelChoicesError;
    use crate::{
        format::{ChatMessage, ChatRequest, StreamingResponse, StreamingResponseReceiver},
        provider_lookup::{ModelLookupChoice, ModelLookupResult},
        request::{try_model_choices, RetryOptions, TryModelChoicesResult},
    };

    async fn test_request(
        choices: Vec<ModelLookupChoice>,
    ) -> Result<(TryModelChoicesResult, StreamingResponseReceiver), TryModelChoicesError> {
        let (chunk_tx, chunk_rx) = flume::bounded(5);
        let res = try_model_choices(
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
                    ..Default::default()
                }],
                ..Default::default()
            },
            chunk_tx,
        )
        .await?;
        Ok((res, chunk_rx))
    }

    async fn test_response(chunk_rx: StreamingResponseReceiver) {
        let chunk = chunk_rx.recv_async().await.unwrap().unwrap();
        match chunk {
            StreamingResponse::Single(res) => {
                assert_eq!(
                    res.choices[0].message.content.as_deref().unwrap(),
                    "A response"
                );
            }
            _ => panic!("Unexpected chunk {chunk:?}"),
        }
    }

    mod single_choice {
        use std::sync::Arc;

        use super::test_request;
        use crate::{provider_lookup::ModelLookupChoice, testing::TestProvider};

        #[tokio::test(start_paused = true)]
        async fn success() {
            let (result, chunk_rx) = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: TestProvider::default().into(),
                api_key: None,
            }])
            .await
            .expect("Failed");

            assert_eq!(result.num_retries, 0);
            assert_eq!(result.was_rate_limited, false);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model");

            super::test_response(chunk_rx).await;
        }

        #[tokio::test(start_paused = true)]
        async fn nonretryable_failures() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::BadRequest),
                ..Default::default()
            });
            let result = test_request(vec![ModelLookupChoice {
                model: "test-model".to_string(),
                provider: provider.clone(),
                api_key: None,
            }])
            .await
            .expect_err("Should have failed");

            assert_eq!(provider.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
            assert_eq!(result.num_retries, 0);
            assert_eq!(result.was_rate_limited, false);
        }

        #[tokio::test(start_paused = true)]
        async fn transient_failure() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::Transient),
                fail_times: 2,
                ..Default::default()
            });
            let (result, chunk_rx) = test_request(vec![ModelLookupChoice {
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
            assert_eq!(result.num_retries, 2);
            assert_eq!(result.was_rate_limited, false);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model");
            super::test_response(chunk_rx).await;
        }

        #[tokio::test(start_paused = true)]
        async fn rate_limit() {
            let provider = Arc::new(TestProvider {
                fail: Some(crate::testing::TestFailure::RateLimit),
                fail_times: 2,
                ..Default::default()
            });
            let (result, chunk_rx) = test_request(vec![ModelLookupChoice {
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
            assert_eq!(result.num_retries, 2);
            assert_eq!(result.was_rate_limited, true);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model");
            super::test_response(chunk_rx).await;
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
            let (result, chunk_rx) = test_request(vec![
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

            assert_eq!(result.num_retries, 0);
            assert_eq!(result.was_rate_limited, false);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model");
            super::test_response(chunk_rx).await;
        }

        #[tokio::test(start_paused = true)]
        async fn transient_failures() {
            let (result, chunk_rx) = test_request(vec![
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

            assert_eq!(result.num_retries, 2);
            assert_eq!(result.was_rate_limited, false);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model-3");
            super::test_response(chunk_rx).await;
        }

        #[tokio::test(start_paused = true)]
        async fn rate_limit() {
            let (result, chunk_rx) = test_request(vec![
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

            assert_eq!(result.num_retries, 1);
            assert_eq!(result.was_rate_limited, true);
            assert_eq!(result.provider, "test");
            assert_eq!(result.model, "test-model-2");
            super::test_response(chunk_rx).await;
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

            let (result, _) = test_request(vec![
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

            assert_eq!(result.num_retries, 3);
            assert_eq!(result.was_rate_limited, true);
            assert_eq!(result.provider, "test");
            // Should have wrapped around to the first one again.
            assert_eq!(result.model, "test-model");
            assert_eq!(p1.calls.load(std::sync::atomic::Ordering::Relaxed), 2);
            assert_eq!(p2.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
            assert_eq!(p3.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        }
    }
}
