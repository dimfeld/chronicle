use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use chronicle_proxy::{
    format::ChatRequest, EventPayload, Proxy, ProxyRequestInternalMetadata, ProxyRequestMetadata,
    ProxyRequestOptions,
};
use error_stack::{Report, ResultExt};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{config::LocalConfig, Error};

pub async fn build_proxy(
    pool: Option<SqlitePool>,
    configs: Vec<(PathBuf, LocalConfig)>,
) -> Result<Proxy, Report<Error>> {
    let mut builder = Proxy::builder();

    if let Some(pool) = pool {
        chronicle_proxy::database::migrations::run_default_migrations(&pool)
            .await
            .change_context(Error::DbInit)?;

        builder = builder
            .with_database(pool)
            .log_to_database(true)
            .load_config_from_database(true);
    }

    for (_, config) in configs {
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

async fn proxy_request(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(mut body): Json<ProxyRequestPayload>,
) -> Result<Response, crate::Error> {
    body.options
        .merge_request_headers(&headers)
        .change_context(Error::InvalidProxyHeader)?;

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
}
