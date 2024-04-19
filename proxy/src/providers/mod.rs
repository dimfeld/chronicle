pub mod anthropic;
pub mod custom;
pub mod ollama;
pub mod openai;

use std::fmt::Debug;

use error_stack::Report;
use reqwest::{StatusCode, Url};
use thiserror::Error;

use crate::{Error, ProxyRequestOptions};

#[async_trait::async_trait]
pub trait ChatModelProvider: Debug + Send + Sync {
    fn name(&self) -> &str;

    /// Send a request and return the response. If there's any chance of retryable failures for
    /// this provider (e.g. almost every provider), then this function should handle retrying with
    /// the behavior specified in `options.retry`. The `retryable_request` function can assist with that.
    async fn send_request(
        &self,
        options: &ProxyRequestOptions,
        body: serde_json::Value,
    ) -> Result<ProviderResponse, Report<Error>>;

    fn default_url(&self) -> &'static str;

    fn is_default_for_model(&self, model: &str) -> bool;
}

/// A generic structure with a provider's response translated into the common format, and possible error codes.
pub struct ProviderResponse {
    // todo use strong typing here?
    pub body: Option<serde_json::Value>,
    /// Any other metadata from the provider that should be logged.
    pub meta: Option<serde_json::Value>,
    /// How many retries were performed.
    pub retries: u32,
    /// True if this request had to be retried due to rate limits.
    pub rate_limited: bool,
    /// The latency of the request. If the request was retried this should only count the
    /// final successful one. Total latency including retries is tracked outside of the provider.
    pub latency: std::time::Duration,
    pub tokens_input: Option<u32>,
    pub tokens_output: Option<u32>,
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct ProviderError {
    /// What type of error this is
    pub kind: ProviderErrorKind,
    /// The HTTP status code, if there was one.
    pub status_code: Option<reqwest::StatusCode>,
    /// The returned body, if there was one
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Error)]
pub enum ProviderErrorKind {
    /// A generic error not otherwise specified. These will be retried
    #[error("Model provider returned an error")]
    Generic,
    /// An error expected to be transient, such as 5xx status codes
    #[error("Model provider encountered a transient error")]
    Transient,

    /// The provider returned a rate limit error.
    #[error("Model provider rate limited this request")]
    RateLimit {
        /// How soon we can retry, if the response specified
        retry_after: Option<std::time::Duration>,
    },

    /// Some non-retryable error not covered below
    #[error("Model provider encountered an unrecoverable error")]
    Permanent,
    /// The model provider didn't like our input
    #[error("Model provider rejected the request format")]
    BadInput,
    /// The API token was rejected or not allowed to perform the requested operation
    #[error("Model provider authorization error")]
    AuthRejected,
    /// The provider needs more money.
    #[error("Out of credits with this provider")]
    OutOfCredits,
}

impl ProviderErrorKind {
    /// Convert an HTTP status code into a `ProviderError`. Returns `None` if the request
    /// succeeded.
    pub fn from_status_code(code: reqwest::StatusCode) -> Option<Self> {
        if code.is_success() {
            return None;
        }

        let code = match code {
            // 5xx errors tend to be transient and retryable
            c if c.is_server_error() => Self::Transient,
            // We don't have the information on how long to wait here, but it can be extracted
            // later by the provider if it is present.
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimit { retry_after: None },
            // Not all providers will return a 402, but if we do see one then it's `OutOfCredits`.
            StatusCode::PAYMENT_REQUIRED => Self::OutOfCredits,
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => Self::AuthRejected,
            StatusCode::BAD_REQUEST
            | StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
            | StatusCode::UNPROCESSABLE_ENTITY
            | StatusCode::UNSUPPORTED_MEDIA_TYPE
            | StatusCode::PAYLOAD_TOO_LARGE
            | StatusCode::NOT_FOUND
            | StatusCode::METHOD_NOT_ALLOWED
            | StatusCode::NOT_ACCEPTABLE => Self::BadInput,
            c if c.is_client_error() => Self::Permanent,
            c => Self::Generic,
        };

        Some(code)
    }

    /// If the request is retryable after a short delay.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Transient | Self::RateLimit { .. } | Self::Generic
        )
    }
}
