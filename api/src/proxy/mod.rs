pub mod build;

use std::time::Duration;

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json, Router,
};
use chronicle_proxy::{
    format::ChatRequest, request::RetryOptions, ModelAndProvider, Proxy,
    ProxyRequestInternalMetadata, ProxyRequestMetadata, ProxyRequestOptions,
};
use error_stack::{Report, ResultExt};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{auth::Authed, server::ServerState, Error};

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    api_key: Option<String>,

    /// Force a certain provider
    provider: Option<String>,

    #[serde(default)]
    models: Vec<ModelAndProvider>,
    #[serde(default)]
    random_choice: bool,

    /// Customize retry behavior
    retry: Option<RetryOptions>,
    /// Metadata about the request, which will be recorded
    meta: Option<ProxyRequestMetadata>,
    /// Timeout, in milliseconds
    timeout: Option<u64>,
}

async fn proxy_request(
    State(state): State<ServerState>,
    auth: Option<Authed>,
    headers: HeaderMap,
    Json(body): Json<ProxyRequestPayload>,
) -> Result<Response, Error> {
    let api_key = body.api_key.or_else(|| {
        headers
            .get("x-provider-api-key")
            .and_then(|s| s.to_str().ok())
            .map(|s| s.to_string())
    });

    // Parse out model and provider choice
    let result = state
        .proxy
        .send(
            ProxyRequestOptions {
                model: None,
                api_key,
                // Don't need this when we're using send_to_provider
                provider: body.provider,
                models: body.models,
                random_choice: body.random_choice,
                retry: body.retry.unwrap_or_default(),
                metadata: body.meta.unwrap_or_default(),
                internal_metadata: ProxyRequestInternalMetadata {
                    organization_id: auth
                        .as_ref()
                        .map(|a| a.organization_id.as_uuid().to_string()),
                    user_id: auth.as_ref().map(|a| a.user_id.as_uuid().to_string()),
                    project_id: None,
                },
                timeout: body.timeout.map(Duration::from_millis),
            },
            body.request,
        )
        .await
        .change_context(Error::Proxy)?;

    Ok(Json(result).into_response())
}

pub fn create_routes() -> Router<ServerState> {
    Router::new()
        .route("/chat", axum::routing::post(proxy_request))
        // We don't use the wildcard path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/char/*path", axum::routing::post(proxy_request))
}
