pub mod anthropic;
pub mod anyscale;
#[cfg(feature = "aws-bedrock")]
pub mod aws_bedrock;
pub mod custom;
pub mod deepinfra;
pub mod fireworks;
pub mod groq;
pub mod mistral;
pub mod ollama;
pub mod openai;
pub mod together;

use std::{fmt::Debug, time::Duration};

use error_stack::Report;
use reqwest::StatusCode;
use thiserror::Error;

use crate::format::{ChatRequest, StreamingResponseSender};

#[derive(Debug)]
pub struct SendRequestOptions {
    pub timeout: Duration,
    pub override_url: Option<String>,
    pub api_key: Option<String>,
    pub body: ChatRequest,
}

#[async_trait::async_trait]
pub trait ChatModelProvider: Debug + Send + Sync {
    /// Internal name for the provider
    fn name(&self) -> &str;

    /// A readable name for the provider
    fn label(&self) -> &str;

    /// Send a request and return the response. If there's any chance of retryable failures for
    /// this provider (e.g. almost every provider), then this function should handle retrying with
    /// the behavior specified in `options.retry`. The `request_with_retry` function can assist with that.
    async fn send_request(
        &self,
        options: SendRequestOptions,
        chunk_tx: StreamingResponseSender,
    ) -> Result<(), Report<ProviderError>>;

    fn is_default_for_model(&self, model: &str) -> bool;
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
    /// How much time it took before we received the error
    pub latency: std::time::Duration,
}

impl ProviderError {
    /// A simple constructor for a [ProviderError] that only needs a kind
    pub fn from_kind(kind: ProviderErrorKind) -> Self {
        Self {
            kind,
            status_code: None,
            body: None,
            latency: std::time::Duration::ZERO,
        }
    }

    /// A helper for creating a `ProviderError` with the TransformingRequest error kind. This is by
    /// far the most common case in the codebase.
    pub fn transforming_request() -> Self {
        Self::from_kind(ProviderErrorKind::TransformingRequest)
    }
}

#[cfg(feature = "filigree")]
impl filigree::errors::HttpError for ProviderError {
    type Detail = serde_json::Value;

    fn status_code(&self) -> StatusCode {
        let Some(status_code) = self.status_code else {
            return StatusCode::INTERNAL_SERVER_ERROR;
        };

        if status_code.is_success() {
            self.kind.status_code()
        } else {
            status_code
        }
    }

    fn error_kind(&self) -> &'static str {
        self.kind.as_str()
    }

    fn error_detail(&self) -> Self::Detail {
        self.body.clone().unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Error)]
pub enum ProviderErrorKind {
    /// A generic error not otherwise specified. These will be retried
    #[error("Model provider returned an error")]
    Generic,
    /// a 5xx HTTP status code or similar error
    #[error("Model provider encountered a server error")]
    Server,
    #[error("Failed while trying to send request")]
    Sending,
    #[error("Failed while parsing response")]
    ParsingResponse,
    #[error("Error transforming a model request")]
    TransformingRequest,
    #[error("Error transforming a model response")]
    TransformingResponse,
    #[error("Provider closed connection prematurely")]
    ProviderClosedConnection,
    /// The provider returned a rate limit error.
    #[error("Model provider rate limited this request")]
    RateLimit {
        /// How soon we can retry, if the response specified
        retry_after: Option<std::time::Duration>,
    },

    /// The request took longer than the conifgured timeout
    #[error("Timed out waiting for model provider's response")]
    Timeout,

    /// Some non-retryable error not covered below
    #[error("Model provider encountered an unrecoverable error")]
    Permanent,
    /// The model provider didn't like our input
    #[error("Model provider rejected the request format")]
    BadInput,
    /// The API token was rejected or not allowed to perform the requested operation
    #[error("Model provider authorization error")]
    AuthRejected,
    /// The API token was rejected or not allowed to perform the requested operation
    #[error("No API key provided")]
    AuthMissing,
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
            c if c.is_server_error() => Self::Server,
            // Some other client error but these tend to indicate that a retry won't work.
            c if c.is_client_error() => Self::Permanent,
            _ => Self::Generic,
        };

        Some(code)
    }

    pub fn from_reqwest_send_error(error: &reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Sending
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            ProviderErrorKind::Generic => StatusCode::INTERNAL_SERVER_ERROR,
            ProviderErrorKind::Server => StatusCode::SERVICE_UNAVAILABLE,
            ProviderErrorKind::Sending => StatusCode::BAD_GATEWAY,
            ProviderErrorKind::ParsingResponse => StatusCode::BAD_GATEWAY,
            ProviderErrorKind::ProviderClosedConnection => StatusCode::BAD_GATEWAY,
            ProviderErrorKind::RateLimit { .. } => StatusCode::TOO_MANY_REQUESTS,
            ProviderErrorKind::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ProviderErrorKind::Permanent => StatusCode::INTERNAL_SERVER_ERROR,
            ProviderErrorKind::BadInput => StatusCode::UNPROCESSABLE_ENTITY,
            ProviderErrorKind::AuthRejected => StatusCode::UNAUTHORIZED,
            ProviderErrorKind::AuthMissing => StatusCode::UNAUTHORIZED,
            ProviderErrorKind::OutOfCredits => StatusCode::PAYMENT_REQUIRED,
            ProviderErrorKind::TransformingRequest => StatusCode::BAD_REQUEST,
            ProviderErrorKind::TransformingResponse => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// If the request is retryable after a short delay.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Server
                | Self::ParsingResponse
                | Self::TransformingResponse
                | Self::Sending
                | Self::RateLimit { .. }
                | Self::Generic
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderErrorKind::Generic => "generic",
            ProviderErrorKind::Server => "provider_server_error",
            ProviderErrorKind::ProviderClosedConnection => "provider_connection_closed",
            ProviderErrorKind::Sending => "provider_connection_error",
            ProviderErrorKind::ParsingResponse => "parsing_provider_response",
            ProviderErrorKind::RateLimit { .. } => "rate_limit",
            ProviderErrorKind::Timeout => "timeout",
            ProviderErrorKind::Permanent => "unrecoverable_server_error",
            ProviderErrorKind::BadInput => "provider_rejected_input",
            ProviderErrorKind::AuthRejected => "provider_rejected_token",
            ProviderErrorKind::AuthMissing => "auth_missing",
            ProviderErrorKind::OutOfCredits => "out_of_credits",
            ProviderErrorKind::TransformingRequest => "transforming_request",
            ProviderErrorKind::TransformingResponse => "transforming_response",
        }
    }
}
