use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, Json};
use chronicle_proxy::{workflow_events::WorkflowEvent, ProxyRequestMetadata};
use error_stack::ResultExt;
use http::{HeaderMap, StatusCode};
use serde::Deserialize;
use smallvec::{smallvec, SmallVec};

use crate::{error::Error, proxy::ServerState};

async fn record_event(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<WorkflowEvent>,
) -> Result<impl IntoResponse, Error> {
    match &mut body {
        WorkflowEvent::RunStart(event) => {
            let mut metadata = ProxyRequestMetadata::default();
            metadata
                .merge_request_headers(&headers)
                .change_context(Error::InvalidProxyHeader)?;
            event.merge_metadata(&metadata);
        }
        _ => {}
    }

    state.proxy.record_event_batch(smallvec![body]).await;

    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct EventsPayload {
    events: SmallVec<[WorkflowEvent; 1]>,
}

async fn record_events(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<EventsPayload>,
) -> Result<impl IntoResponse, Error> {
    let mut metadata = ProxyRequestMetadata::default();
    metadata
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

    for event in &mut body.events {
        match event {
            WorkflowEvent::RunStart(event) => {
                event.merge_metadata(&metadata);
            }
            _ => {}
        }
    }

    state.proxy.record_event_batch(body.events).await;

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
        .route(
            "/healthz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
}
