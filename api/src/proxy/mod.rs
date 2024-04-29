pub mod build;

use std::{str::FromStr, time::Duration};

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
use serde::{de::DeserializeOwned, Deserialize};

use crate::{auth::Authed, server::ServerState, Error};

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    api_key: Option<String>,

    /// Force a certain provider
    provider: Option<String>,

    models: Option<Vec<ModelAndProvider>>,
    random_choice: Option<bool>,

    /// Customize retry behavior
    retry: Option<RetryOptions>,
    /// Metadata about the request, which will be recorded
    meta: Option<ProxyRequestMetadata>,
    /// Timeout, in milliseconds
    timeout: Option<u64>,
}

fn get_header_str(body_value: Option<String>, headers: &HeaderMap, key: &str) -> Option<String> {
    if body_value.is_some() {
        return body_value;
    }

    headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| s.to_string())
}

fn get_header_t<T>(
    body_value: Option<T>,
    headers: &HeaderMap,
    key: &str,
) -> Result<Option<T>, Report<Error>>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    if body_value.is_some() {
        return Ok(body_value);
    }

    headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| s.parse::<T>())
        .transpose()
        .change_context(Error::InvalidProxyHeader(key.to_string()))
}

fn get_header_json<T: DeserializeOwned>(
    body_value: Option<T>,
    headers: &HeaderMap,
    key: &str,
) -> Result<Option<T>, Report<Error>> {
    if body_value.is_some() {
        return Ok(body_value);
    }

    headers
        .get(key)
        .and_then(|s| s.to_str().ok())
        .map(|s| serde_json::from_str(s))
        .transpose()
        .change_context(Error::InvalidProxyHeader(key.to_string()))
}

async fn proxy_request(
    State(state): State<ServerState>,
    auth: Option<Authed>,
    headers: HeaderMap,
    Json(body): Json<ProxyRequestPayload>,
) -> Result<Response, Error> {
    let api_key = get_header_str(body.api_key, &headers, "x-chronicle-provider-api-key");
    let provider = get_header_str(body.provider, &headers, "x-chronicle-provider");
    let models = get_header_json(body.models, &headers, "x-chronicle-models")?;
    let random_choice = get_header_t(body.random_choice, &headers, "x-chronicle-random-choice")?;
    let retry = get_header_json(body.retry, &headers, "x-chronicle-retry")?;
    let meta = get_header_json(body.meta, &headers, "x-chronicle-meta")?;
    let timeout = get_header_t(body.timeout, &headers, "x-chronicle-timeout")?;

    let result = state
        .proxy
        .send(
            ProxyRequestOptions {
                model: None,
                api_key,
                provider,
                models: models.unwrap_or_default(),
                random_choice: random_choice.unwrap_or_default(),
                retry: retry.unwrap_or_default(),
                metadata: meta.unwrap_or_default(),
                internal_metadata: ProxyRequestInternalMetadata {
                    organization_id: auth
                        .as_ref()
                        .map(|a| a.organization_id.as_uuid().to_string()),
                    user_id: auth.as_ref().map(|a| a.user_id.as_uuid().to_string()),
                    project_id: None,
                },
                timeout: timeout.map(Duration::from_millis),
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
        .route("/chat/*path", axum::routing::post(proxy_request))
        .route("/v1/chat/*path", axum::routing::post(proxy_request))
}
