use axum::response::{IntoResponse, Response};
use error_stack::{Report, ResultExt};
use filigree::errors::HttpError;
use http::StatusCode;
use serde_json::json;
use thiserror::Error;

/// The top-level error type from the platform
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to initialize database
    #[error("Failed to initialize database")]
    DbInit,
    /// Database error not otherwise handled
    #[error("Database error")]
    Db,
    /// Configuration error
    #[error("Configuration error")]
    Config,
    /// Failed to start the HTTP server
    #[error("Failed to start server")]
    ServerStart,
    /// Failure while shutting down
    #[error("Encountered error while shutting down")]
    Shutdown,
    /// The requested item was not found
    #[error("{0} not found")]
    NotFound(&'static str),
    /// A wrapper around a Report<Error> to let it be returned from an Axum handler, since we can't
    /// implement IntoResponse on Report
    #[error("{0}")]
    WrapReport(Report<Error>),
    #[error("Missing Model")]
    MissingModel,
    #[error("Missing provider for model {0}")]
    MissingProvider(String),

    #[error("Model provider error")]
    Proxy,
    #[error("Failed to build proxy")]
    BuildingProxy,
    #[error("Failed to read proxy request options")]
    InvalidProxyHeader,
}

impl From<Report<Error>> for Error {
    fn from(value: Report<Error>) -> Self {
        Error::WrapReport(value)
    }
}

impl Error {
    /// If this Error contains a Report<Error>, find an inner HttpError whose error data we may want to use.
    fn find_downstack_error_code(&self) -> Option<StatusCode> {
        let Error::WrapReport(report) = self else {
            return None;
        };

        report.frames().find_map(|frame| {
            filigree::downref_report_frame!(
                frame,
                |e| e.status_code(),
                chronicle_proxy::providers::ProviderError
            )
        })
    }

    /// If this Error contains a Report<Error>, find an inner HttpError whose error data we may want to use.
    fn find_downstack_error_kind(&self) -> Option<&'static str> {
        let Error::WrapReport(report) = self else {
            return None;
        };

        report.frames().find_map(|frame| {
            filigree::downref_report_frame!(
                frame,
                |e| e.error_kind(),
                chronicle_proxy::providers::ProviderError
            )
        })
    }

    fn find_downstack_error_detail(&self) -> Option<serde_json::Value> {
        let Error::WrapReport(report) = self else {
            return None;
        };

        report.frames().find_map(|frame| {
            frame
                .downcast_ref::<chronicle_proxy::providers::ProviderError>()
                .and_then(|e| e.body.clone())
        })
    }
}

impl HttpError for Error {
    type Detail = serde_json::Value;

    fn error_kind(&self) -> &'static str {
        if let Some(error_kind) = self.find_downstack_error_kind() {
            return error_kind;
        }

        match self {
            Error::WrapReport(e) => e.current_context().error_kind(),
            Error::DbInit => "db_init",
            Error::Db => "db",
            Error::ServerStart => "server_start",
            Error::NotFound(_) => "not_found",
            Error::Shutdown => "shutdown",
            Error::MissingModel => "missing_model",
            Error::MissingProvider(_) => "missing_provider",
            Error::Proxy => "proxy",
            Error::BuildingProxy => "building_proxy",
            Error::InvalidProxyHeader => "invalid_proxy_headers",
            Error::Config => "config",
        }
    }

    fn status_code(&self) -> StatusCode {
        if let Some(status_code) = self.find_downstack_error_code() {
            return status_code;
        }

        match self {
            Error::WrapReport(e) => e.current_context().status_code(),
            Error::DbInit => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Db => StatusCode::INTERNAL_SERVER_ERROR,
            Error::ServerStart => StatusCode::INTERNAL_SERVER_ERROR,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Shutdown => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Config => StatusCode::INTERNAL_SERVER_ERROR,
            Error::MissingModel => StatusCode::BAD_REQUEST,
            Error::MissingProvider(_) => StatusCode::BAD_REQUEST,
            Error::InvalidProxyHeader => StatusCode::UNPROCESSABLE_ENTITY,
            Error::Proxy => StatusCode::INTERNAL_SERVER_ERROR,
            Error::BuildingProxy => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_detail(&self) -> serde_json::Value {
        let body = self.find_downstack_error_detail();

        let error_details = match self {
            Error::WrapReport(e) => e.error_detail().into(),
            _ => serde_json::Value::Null,
        };

        json!({
            "body": body,
            "details": error_details,
        })
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.to_response()
    }
}
