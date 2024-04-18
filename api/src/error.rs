use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use error_stack::Report;
use filigree::{
    auth::AuthError,
    errors::{ErrorKind as FilErrorKind, ForceObfuscate, HttpError},
    storage::StorageError,
    uploads::UploadInspectorError,
};
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
    /// Task queue error not otherwise handled
    #[error("Task Queue error")]
    TaskQueue,
    /// Failed to start the HTTP server
    #[error("Failed to start server")]
    ServerStart,
    /// Failure while shutting down
    #[error("Encountered error while shutting down")]
    Shutdown,
    /// Error running a scheduled task
    #[error("Error running scheduled task")]
    ScheduledTask,
    /// The requested item was not found
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("Invalid filter")]
    Filter,
    #[error("Failed to upload file")]
    Upload,
    #[error("Error communicating with object storage")]
    Storage,
    /// A wrapper around a Report<Error> to let it be returned from an Axum handler, since we can't
    /// implement IntoResponse on Report
    #[error("{0}")]
    WrapReport(Report<Error>),
    #[error("Missing Permission {0}")]
    MissingPermission(&'static str),
    #[error(transparent)]
    AuthError(#[from] filigree::auth::AuthError),
    #[error("Auth subsystem error")]
    AuthSubsystem,
    #[error("Login failure")]
    Login,
    /// An invalid Host header was passed
    #[error("Invalid host")]
    InvalidHostHeader,
    #[error("Type Export Error")]
    TypeExport,
    #[error("Missing Model")]
    MissingModel,
    #[error("Missing provider for model {0}")]
    MissingProvider(String),
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
                AuthError,
                UploadInspectorError,
                StorageError
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
                AuthError,
                UploadInspectorError,
                StorageError
            )
        })
    }

    /// If this Error contains a Report<Error>, find an inner HttpError whose error data we may want to use.
    fn find_downstack_error_obfuscate(&self) -> Option<Option<ForceObfuscate>> {
        let Error::WrapReport(report) = self else {
            return None;
        };

        report.frames().find_map(|frame| {
            filigree::downref_report_frame!(
                frame,
                |e| e.obfuscate(),
                AuthError,
                UploadInspectorError
            )
        })
    }
}

impl HttpError for Error {
    type Detail = String;

    fn error_kind(&self) -> &'static str {
        if let Some(error_kind) = self.find_downstack_error_kind() {
            return error_kind;
        }

        match self {
            Error::WrapReport(e) => e.current_context().error_kind(),
            Error::Upload => FilErrorKind::UploadFailed.as_str(),
            Error::DbInit => FilErrorKind::DatabaseInit.as_str(),
            Error::Db => FilErrorKind::Database.as_str(),
            Error::TaskQueue => ErrorKind::TaskQueue.as_str(),
            Error::ServerStart => FilErrorKind::ServerStart.as_str(),
            Error::NotFound(_) => FilErrorKind::NotFound.as_str(),
            Error::Shutdown => FilErrorKind::Shutdown.as_str(),
            Error::ScheduledTask => ErrorKind::ScheduledTask.as_str(),
            Error::Filter => ErrorKind::Filter.as_str(),
            Error::AuthError(e) => e.error_kind(),
            Error::AuthSubsystem => ErrorKind::AuthSubsystem.as_str(),
            Error::Login => FilErrorKind::Unauthenticated.as_str(),
            Error::MissingPermission(_) => FilErrorKind::Unauthenticated.as_str(),
            Error::InvalidHostHeader => FilErrorKind::InvalidHostHeader.as_str(),
            Error::Storage => FilErrorKind::Storage.as_str(),
            Error::MissingModel => "missing_model",
            Error::MissingProvider(_) => "missing_provider",
            // These aren't ever returned, we just need some value to fill out the match
            Error::Config => "config",
            Error::TypeExport => "cli",
        }
    }

    fn obfuscate(&self) -> Option<ForceObfuscate> {
        if let Some(obfuscate) = self.find_downstack_error_obfuscate() {
            return obfuscate;
        }

        match self {
            Error::InvalidHostHeader => Some(ForceObfuscate::new(
                FilErrorKind::BadRequest,
                "Invalid Request",
            )),
            Error::AuthError(e) => e.obfuscate(),
            _ => None,
        }
    }

    fn status_code(&self) -> StatusCode {
        if let Some(status_code) = self.find_downstack_error_code() {
            return status_code;
        }

        match self {
            Error::WrapReport(e) => e.current_context().status_code(),
            Error::AuthError(e) => e.status_code(),
            Error::Upload => StatusCode::INTERNAL_SERVER_ERROR,
            Error::DbInit => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Db => StatusCode::INTERNAL_SERVER_ERROR,
            Error::TaskQueue => StatusCode::INTERNAL_SERVER_ERROR,
            Error::ServerStart => StatusCode::INTERNAL_SERVER_ERROR,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Shutdown => StatusCode::INTERNAL_SERVER_ERROR,
            Error::ScheduledTask => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Filter => StatusCode::BAD_REQUEST,
            Error::AuthSubsystem => StatusCode::INTERNAL_SERVER_ERROR,
            Error::MissingPermission(_) => StatusCode::FORBIDDEN,
            Error::Login => StatusCode::UNAUTHORIZED,
            Error::InvalidHostHeader => StatusCode::BAD_REQUEST,
            Error::Storage => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Config => StatusCode::INTERNAL_SERVER_ERROR,
            Error::TypeExport => StatusCode::INTERNAL_SERVER_ERROR,
            Error::MissingModel => StatusCode::BAD_REQUEST,
            Error::MissingProvider(_) => StatusCode::BAD_REQUEST,
        }
    }

    fn error_detail(&self) -> String {
        match self {
            Error::WrapReport(e) => e.error_detail(),
            _ => String::new(),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.to_response()
    }
}

pub enum ErrorKind {
    TaskQueue,
    ScheduledTask,
    Filter,
    AuthSubsystem,
    Login,
}

impl ErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::TaskQueue => "task_queue",
            ErrorKind::ScheduledTask => "scheduled_task",
            ErrorKind::Filter => "invalid_filter",
            ErrorKind::AuthSubsystem => "auth",
            ErrorKind::Login => "auth",
        }
    }
}
