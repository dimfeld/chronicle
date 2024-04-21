use std::time::Duration;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json, Router,
};
use chronicle_proxy::{
    format::ChatRequest, request::RetryOptions, ProxyRequestInternalMetadata, ProxyRequestMetadata,
    ProxyRequestOptions,
};
use error_stack::ResultExt;
use serde::Deserialize;

use crate::{auth::Authed, server::ServerState, Error};

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    /// Force a certain provider
    provider: Option<String>,
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
    Json(body): Json<ProxyRequestPayload>,
) -> Result<Response, Error> {
    let model = body
        .request
        .model
        .as_deref()
        .ok_or(Error::MissingModel)?
        .to_string();
    let provider = body
        .provider
        .as_deref()
        .and_then(|s| state.proxy.get_provider(s))
        .or_else(|| state.proxy.default_provider_for_model(&model))
        .ok_or_else(|| Error::MissingProvider(model.to_string()))?;

    // Parse out model and provider choice
    let result = state
        .proxy
        .send_to_provider(
            provider,
            ProxyRequestOptions {
                model: Some(model),
                // Don't need this when we're using send_to_provider
                provider: None,
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
