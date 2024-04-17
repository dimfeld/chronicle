use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing,
};
use filigree::{
    auth::password::{login_with_password, EmailAndPassword},
    extract::FormOrJson,
};
use maud::html;
use schemars::JsonSchema;

use crate::{
    auth::{has_any_permission, Authed},
    pages::{error::HtmlError, layout::root_layout_page},
    server::ServerState,
};

#[derive(serde::Deserialize, Debug)]
struct RedirectTo {
    redirect_to: Option<String>,
}

async fn login_form(
    State(state): State<ServerState>,
    Query(query): Query<RedirectTo>,
    FormOrJson(payload): FormOrJson<EmailAndPassword>,
) -> impl IntoResponse {
    html! {}
}

async fn login_page(State(state): State<ServerState>) -> impl IntoResponse {
    root_layout_page(None, "Login", html! { h1 { "Login" } })
}

pub fn create_routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route("/login", routing::get(login_page))
        .route("/login", routing::post(login_form))
}
