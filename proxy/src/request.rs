//! Utilities for retrying a provider request as needed.
use std::time::Duration;

use bytes::Bytes;
use error_stack::{Report, ResultExt};
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::instrument;

use crate::{
    format::{ChatRequest, ChatResponse},
    providers::{ProviderError, ProviderErrorKind, SendRequestOptions},
    Error, ModelLookupChoice, ModelLookupResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryOptions {
    /// How long to wait after the first failure.
    /// The default value is 200ms.
    initial_backoff: Duration,

    /// How to increase the backoff duration as additional retries occur. The default value
    /// is an exponential backoff with a multiplier of `2.0`.
    increase: RepeatBackoffBehavior,

    /// The number of times to try the request, including the first try.
    /// Defaults to 4.
    max_tries: u32,

    /// Maximum amount of jitter to add. The added jitter will be a random value between 0 and this
    /// value.
    /// Defaults to 100ms.
    jitter: Duration,

    /// Never wait more than this amount of time. The behavior of this flag may be modified by the
    /// `fail_if_rate_limit_exceeds_max_backoff` flag.
    max_backoff: Duration,

    /// If a rate limit response asks to wait longer than the `max_backoff`, then stop retrying.
    /// Otherwise it will wait for the requested time even if it is longer than max_backoff.
    /// Defaults to true.
    fail_if_rate_limit_exceeds_max_backoff: bool,
}

impl Default for RetryOptions {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_millis(200),
            increase: RepeatBackoffBehavior::Exponential { multiplier: 2.0 },
            max_backoff: Duration::from_millis(5000),
            max_tries: 4,
            jitter: Duration::from_millis(100),
            fail_if_rate_limit_exceeds_max_backoff: true,
        }
    }
}

/// How to increase the backoff duration as additional retries occur.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(tag = "type")]
pub enum RepeatBackoffBehavior {
    /// Use the initial backoff duration for additional retries as well.
    Constant,
    /// Add this duration to the backoff duration after each retry.
    Additive { amount: Duration },
    /// Multiply the backoff duration by this value after each retry.
    Exponential { multiplier: f64 },
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
pub struct ProxiedResult {
    pub body: ChatResponse,
    /// Any other metadata from the provider that should be logged.
    pub meta: Option<serde_json::Value>,
    /// The latency of the request. If the request was retried this should only count the
    /// final successful one. Total latency including retries is tracked outside of the provider.
    pub latency: std::time::Duration,
    pub num_retries: u32,
    pub was_rate_limited: bool,
}

/// Run a provider request and retry on failure.
#[instrument(level = "debug")]
pub async fn try_model_choices(
    ModelLookupResult {
        alias,
        random,
        choices,
    }: ModelLookupResult,
    options: RetryOptions,
    timeout: Duration,
    request: ChatRequest,
) -> Result<ProxiedResult, Report<Error>> {
    let single_choice = choices.len() == 1;
    let start_choice = if random && !single_choice {
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
                timeout,
                api_key: api_key.clone(),
                body,
            })
            .await;

        let error = match result {
            Ok(value) => {
                return Ok(ProxiedResult {
                    body: value.body,
                    meta: value.meta,
                    latency: value.latency,
                    num_retries: current_try - 1,
                    was_rate_limited,
                })
            }
            Err(e) => {
                tracing::error!(err=?e);
                e
            }
        };

        let Some(provider_error) = error
            .frames()
            .find_map(|frame| frame.downcast_ref::<ProviderError>())
        else {
            // If it wasn't a ProviderError then it's not retryable
            return Err(error);
        };

        if !provider_error.kind.retryable() || current_try == options.max_tries {
            return Err(error);
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
            let wait = backoff.next();
            let wait = match provider_error.kind {
                // Rate limited, where the provider specified a time to wait
                ProviderErrorKind::RateLimit {
                    retry_after: Some(retry_after),
                } => {
                    was_rate_limited = true;

                    if options.fail_if_rate_limit_exceeds_max_backoff
                        && retry_after > options.max_backoff
                    {
                        // Rate limited with a retry time that exceeds max backoff.
                        return Err(error);
                    }

                    // If the rate limit retry duration is more than the planned wait, then wait for
                    // the rate limit duration instead.
                    wait.max(retry_after)
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
pub async fn send_standard_request<RESPONSE: DeserializeOwned>(
    timeout: Duration,
    prepare: impl Fn() -> reqwest::RequestBuilder,
    handle_rate_limit: impl Fn(&reqwest::Response) -> Option<Duration>,
    body: Bytes,
) -> Result<(RESPONSE, Duration), Report<ProviderError>> {
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

        Err(Report::new(ProviderError {
            kind: e,
            status_code: Some(status),
            body,
        }))
    } else {
        // Get the result as text first so that we can save the entire response for better
        // introspection if parsing fails.
        let text = result.text().await.change_context(ProviderError {
            kind: ProviderErrorKind::ParsingResponse,
            status_code: Some(status),
            body: None,
        })?;

        let jd = &mut serde_json::Deserializer::from_str(&text);
        let body: RESPONSE =
            serde_path_to_error::deserialize(jd).change_context(ProviderError {
                kind: ProviderErrorKind::ParsingResponse,
                status_code: Some(status),
                body: Some(serde_json::Value::String(text)),
            })?;

        let latency = start.elapsed();
        Ok::<_, Report<ProviderError>>((body, latency))
    }
}
