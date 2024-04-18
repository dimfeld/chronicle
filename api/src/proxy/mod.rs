use axum::{body::Body, extract::State, response::Response, Json, Router};
use chronicle_proxy::Proxy;
use http::StatusCode;

use crate::{server::ServerState, Error};

async fn proxy_request(
    State(state): State<ServerState>,
    Json(mut body): Json<serde_json::Value>,
) -> Result<Response, Error> {
    // Parse out extra metadata

    // move most of this into the proxy object itself. This endpoint should just be a thing wrapper
    // around that.
    let model = body["model"]
        .as_str()
        .ok_or(Error::MissingModel)?
        .to_string();
    let provider = body["provider"]
        .take()
        .as_str()
        .and_then(|s| state.model_providers.get(&s))
        .or_else(|| state.model_providers.default_for_model(&model))
        .ok_or_else(|| Error::MissingProvider(model.to_string()))?;

    // meta fields:
    // application
    // environment
    // workflow name
    // workflow instance id
    // subtask name
    // arbitrary other data
    let meta = body["meta"].take();

    // Parse out model and provider choice

    // Send update to postgres recorder

    tracing::info!(?body, "Starting request");

    // Run call

    // Get response stats: latency, tokens used, etc.

    let result = ();
    tracing::info!(?result, "Finished request");

    todo!()
}

pub fn create_routes() -> Router<ServerState> {
    Router::new().route("/chat", axum::routing::post(proxy_request))
}
