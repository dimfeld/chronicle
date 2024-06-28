use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use chronicle_proxy::{
    database::logging::ProxyLogEntry,
    workflow_events::{
        ErrorData, RunStartEvent, RunUpdateEvent, StepEndData, StepEvent, StepStartData,
        StepStateData,
    },
    EventPayload, ProxyRequestInternalMetadata, ProxyRequestMetadata,
};
use error_stack::ResultExt;
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::Error, proxy::ServerState};

#[derive(Serialize)]
struct Id {
    id: Uuid,
}

async fn record_event(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<ProxyLogEntry>,
) -> Result<impl IntoResponse, Error> {
    match body {
        ProxyLogEntry::Event(e) => {
            e.metadata
                .merge_request_headers(&headers)
                .change_context(Error::InvalidProxyHeader)?;
        }
        ProxyLogEntry::RunStart(event) => {
            let mut metadata = ProxyRequestMetadata::default();
            metadata
                .merge_request_headers(&headers)
                .change_context(Error::InvalidProxyHeader)?;
            event.merge_metadata(&metadata);
        }
        _ => {}
    }

    state.proxy.record_event_batch(body.into()).await;

    Ok(StatusCode::ACCEPTED)
}

async fn record_events(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<Vec<ProxyLogEntry>>,
) -> Result<impl IntoResponse, Error> {
    let mut metadata = ProxyRequestMetadata::default();
    metadata
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

    for mut event in &mut body {
        event.metadata.merge_from(&metadata);
    }

    let id = state.proxy.record_event_batch(body.into()).await;

    Ok(StatusCode::ACCEPTED)
}

pub fn create_routes() -> axum::Router<Arc<ServerState>> {
    axum::Router::new()
        .route(
            "/",
            axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
        .route("/event", axum::routing::post(record_event))
        .route("/events", axum::routing::post(record_events))
        .route("/v1/event", axum::routing::post(record_event))
        .route("/v1/events", axum::routing::post(record_events))
        .route("/healthz", axum::routing::get(|| async { "OK" }))
}
