use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use chronicle_proxy::{format::ChatRequest, Proxy, ProxyRequestOptions};
use error_stack::{Report, ResultExt};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{config::LocalConfig, Error};

pub async fn build_proxy(
    pool: Option<SqlitePool>,
    load_dotenv: bool,
    configs: Vec<(PathBuf, LocalConfig)>,
) -> Result<Proxy, Report<Error>> {
    let mut builder = Proxy::builder();

    if load_dotenv {
        dotenvy::dotenv().ok();
    }

    for (dir, config) in configs {
        if load_dotenv && config.server_config.dotenv.unwrap_or(true) {
            dotenvy::from_path_override(dir.join(".env")).ok();
        }

        builder = builder.with_config(config.proxy_config);
    }

    if let Some(pool) = pool {
        chronicle_proxy::database::migrations::run_default_migrations(&pool)
            .await
            .change_context(Error::DbInit)?;

        builder = builder
            .with_database(pool)
            .log_to_database(true)
            .load_config_from_database(true);
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

pub fn create_routes() -> axum::Router<Arc<ServerState>> {
    axum::Router::new()
        .route("/chat", axum::routing::post(proxy_request))
        // We don't use the wildcard path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/chat/*path", axum::routing::post(proxy_request))
        .route("/v1/chat/*path", axum::routing::post(proxy_request))
}
