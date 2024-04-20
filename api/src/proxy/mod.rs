use std::time::Duration;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json, Router,
};
use chronicle_proxy::{
    format::ChatRequest, request::RetryOptions, Proxy, ProxyRequestMetadata, ProxyRequestOptions,
};
use error_stack::ResultExt;
use http::StatusCode;
use serde::Deserialize;

use crate::{server::ServerState, Error};

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    provider: Option<String>,
    retry: Option<RetryOptions>,
    meta: Option<ProxyRequestMetadata>,
    /// Timeout, in milliseconds
    timeout: Option<u32>,
}

async fn proxy_request(
    State(state): State<ServerState>,
    Json(body): Json<ProxyRequestPayload>,
) -> Result<Response, Error> {
    // Parse out extra metadata

    // move most of this into the proxy object itself. This endpoint should just be a thing wrapper
    // around that.
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
                timeout: Duration::from_millis(body.timeout.unwrap_or(30_000) as u64),
            },
            body.request,
        )
        .await
        .change_context(Error::Proxy)?;

    Ok(Json(result).into_response())
}

fn value_as_string(value: serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    }
}

pub fn create_routes() -> Router<ServerState> {
    Router::new()
        .route("/chat", axum::routing::post(proxy_request))
        // We dont use this path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/char/*path", axum::routing::post(proxy_request))
}
