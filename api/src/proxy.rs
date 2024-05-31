use std::sync::Arc;

use axum::{
    extract::State,
    http::HeaderMap,
    response::{sse, IntoResponse, Response, Sse},
    Json,
};
use chronicle_proxy::{
    collect_response,
    database::Database,
    format::{ChatRequest, SingleChatResponse, StreamingResponse},
    EventPayload, Proxy, ProxyRequestInternalMetadata, ProxyRequestOptions,
};
use error_stack::{Report, ResultExt};
use futures::StreamExt;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{config::Configs, Error};

pub async fn build_proxy(db: Option<Database>, configs: Configs) -> Result<Proxy, Report<Error>> {
    let mut builder = Proxy::builder();

    if let Some(db) = db {
        builder = builder
            .with_database(db)
            .log_to_database(true)
            .load_config_from_database(true);
    }

    for (_, config) in configs.global.into_iter().chain(configs.cwd.into_iter()) {
        builder = builder.with_config(config.proxy_config);
    }

    builder.build().await.change_context(Error::BuildingProxy)
}

pub struct ServerState {
    pub proxy: Proxy,
}

#[derive(Deserialize, Debug)]
struct ProxyRequestPayload {
    #[serde(flatten)]
    request: ChatRequest,

    #[serde(flatten)]
    options: ProxyRequestOptions,
}

#[derive(Debug, Serialize)]
struct ProxyRequestNonstreamingResult {
    #[serde(flatten)]
    response: SingleChatResponse,
    meta: ProxiedChatResponseMeta,
}

#[derive(Debug, Serialize)]
struct ProxiedChatResponseMeta {
    id: Uuid,
    provider: String,
    response_meta: Option<serde_json::Value>,
    was_rate_limited: bool,
}

async fn proxy_request(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<ProxyRequestPayload>,
) -> Result<Response, crate::Error> {
    body.options
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

    let n = body.request.n.unwrap_or(1) as usize;
    let stream = body.request.stream;
    let result = state
        .proxy
        .send(body.options, body.request)
        .await
        .change_context(Error::Proxy)?;

    if stream {
        let stream = result.into_stream().filter_map(|chunk| async move {
            match chunk {
                Ok(StreamingResponse::Chunk(chunk)) => Some(sse::Event::default().json_data(chunk)),
                Ok(StreamingResponse::Single(chunk)) => {
                    // TODO convert to a delta and send
                    None
                }
                Ok(StreamingResponse::RequestInfo(_) | StreamingResponse::ResponseInfo(_)) => {
                    // Need to figure out if there's some way we can send this along with the
                    // deltas as metadata, but it's difficult since the OpenAI format uses
                    // data-only SSE so we can't just define a new event type or something.
                    // Might work to send an extra chunk with an empty choices but need to see if
                    // that messes things up.
                    None
                }
                Err(e) => {
                    // TODO return the error
                    None
                }
            }
        });

        Ok(Sse::new(stream).into_response())
    } else {
        let result = collect_response(result, n)
            .await
            .change_context(Error::Proxy)?;
        // TODO format this into a ProxyRequestNonstreamingResult
        Ok(Json(result).into_response())
    }
}

#[derive(Serialize)]
struct Id {
    id: Uuid,
}

async fn record_event(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<EventPayload>,
) -> Result<impl IntoResponse, Error> {
    body.metadata
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

    let id = state
        .proxy
        .record_event(ProxyRequestInternalMetadata::default(), body)
        .await;

    Ok((StatusCode::ACCEPTED, Json(Id { id })))
}

pub fn create_routes() -> axum::Router<Arc<ServerState>> {
    axum::Router::new()
        .route("/event", axum::routing::post(record_event))
        .route("/v1/event", axum::routing::post(record_event))
        .route("/chat", axum::routing::post(proxy_request))
        // We don't use the wildcard path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/chat/*path", axum::routing::post(proxy_request))
        .route("/v1/chat/*path", axum::routing::post(proxy_request))
        .route("/healthz", axum::routing::get(|| async { "OK" }))
}
