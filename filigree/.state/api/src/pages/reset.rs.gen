use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing,
};
use filigree::extract::FormOrJson;
use maud::html;
use schemars::JsonSchema;

use crate::{
    auth::{has_any_permission, Authed},
    pages::{error::HtmlError, layout::root_layout_page},
    server::ServerState,
};

#[derive(serde::Deserialize, Debug, JsonSchema)]
pub struct ResetPayload {
    email: String,
}

async fn reset_form(
    State(state): State<ServerState>,
    payload: FormOrJson<ResetPayload>,
) -> Result<impl IntoResponse, HtmlError> {
    Ok(html! {})
}

async fn reset_page(State(state): State<ServerState>) -> impl IntoResponse {
    root_layout_page(None, "Reset", html! { h1 { "Reset" } })
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route("/reset", routing::get(reset_page))
        .route("/reset", routing::post(reset_form))
}
