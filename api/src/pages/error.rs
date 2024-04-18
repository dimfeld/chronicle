use axum::response::{IntoResponse, Response};
use filigree::errors::HttpError;
use http::StatusCode;

use crate::Error;

pub struct HtmlError(pub Error);

impl From<Error> for HtmlError {
    fn from(value: Error) -> Self {
        HtmlError(value)
    }
}

impl From<error_stack::Report<Error>> for HtmlError {
    fn from(value: error_stack::Report<Error>) -> Self {
        HtmlError(Error::WrapReport(value))
    }
}

impl IntoResponse for HtmlError {
    fn into_response(self) -> Response {
        match self.0.status_code() {
            StatusCode::NOT_FOUND => super::not_found::not_found_page(),
            _ => super::generic_error::generic_error_page(&self.0),
        }
    }
}
