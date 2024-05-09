pub mod build;

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json, Router,
};
use chronicle_proxy::{
    format::ChatRequest, EventPayload, ProxyRequestInternalMetadata, ProxyRequestOptions,
};
use error_stack::ResultExt;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::Authed, server::ServerState, Error};

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    #[serde(flatten)]
    options: ProxyRequestOptions,
}

async fn proxy_request(
    State(state): State<ServerState>,
    auth: Option<Authed>,
    headers: HeaderMap,
    Json(mut body): Json<ProxyRequestPayload>,
) -> Result<Response, Error> {
    body.options
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

    body.options.internal_metadata = ProxyRequestInternalMetadata {
        organization_id: auth
            .as_ref()
            .map(|a| a.organization_id.as_uuid().to_string()),
        user_id: auth.as_ref().map(|a| a.user_id.as_uuid().to_string()),
        project_id: None,
    };

    let result = state
        .proxy
        .send(body.options, body.request)
        .await
        .change_context(Error::Proxy)?;

    Ok(Json(result).into_response())
}

#[derive(Serialize)]
struct Id {
    id: Uuid,
}

async fn record_event(
    State(state): State<ServerState>,
    auth: Option<Authed>,
    headers: HeaderMap,
    Json(mut body): Json<EventPayload>,
) -> Result<impl IntoResponse, Error> {
    body.metadata
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;
    let internal_metadata = ProxyRequestInternalMetadata {
        organization_id: auth
            .as_ref()
            .map(|a| a.organization_id.as_uuid().to_string()),
        user_id: auth.as_ref().map(|a| a.user_id.as_uuid().to_string()),
        project_id: None,
    };

    let id = state.proxy.record_event(internal_metadata, body).await;

    Ok((StatusCode::ACCEPTED, Json(Id { id })))
}

pub fn create_routes() -> Router<ServerState> {
    Router::new()
        .route("/event", axum::routing::post(record_event))
        .route("/v1/event", axum::routing::post(record_event))
        .route("/chat", axum::routing::post(proxy_request))
        // We don't use the wildcard path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/chat/*path", axum::routing::post(proxy_request))
        .route("/v1/chat/*path", axum::routing::post(proxy_request))
}
