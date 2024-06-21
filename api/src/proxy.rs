use std::{future::ready, sync::Arc};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{sse, IntoResponse, Response, Sse},
    Json,
};
use chronicle_proxy::{
    collect_response,
    database::Database,
    format::{
        ChatRequest, RequestInfo, SingleChatResponse, StreamingChatResponse, StreamingResponse,
    },
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
    meta: RequestInfo,
}

#[derive(Serialize)]
struct DeltaWithRequestInfo {
    #[serde(flatten)]
    data: StreamingChatResponse,
    meta: RequestInfo,
}

#[derive(Serialize)]
struct OpenAiSseError {
    error: Option<serde_json::Value>,
    message: String,
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
        // The first item will always be a RequestInfo or an error. We pull it off here so that if the
        // model provider returned an error we can catch it in advance and return a proper error.
        let request_info = result
            .recv_async()
            .await
            .change_context(Error::Proxy)
            .attach_printable("Connection terminated unexpectedly")?
            .change_context(Error::Proxy)?;

        let request_info = match request_info {
            StreamingResponse::RequestInfo(info) => Some(info),
            _ => {
                tracing::error!("First stream item was not a RequestInfo");
                None
            }
        };

        let stream = result
            .into_stream()
            .scan(request_info, |request_info, chunk| {
                let result = match chunk {
                    Ok(StreamingResponse::Chunk(chunk)) => {
                        if let Some(info) = request_info.take() {
                            // Attach RequestInfo to the chunk if we have it
                            let chunk = DeltaWithRequestInfo {
                                data: chunk,
                                meta: info,
                            };
                            Some(sse::Event::default().json_data(chunk))
                        } else {
                            Some(sse::Event::default().json_data(chunk))
                        }
                    }
                    Ok(StreamingResponse::Single(chunk)) => {
                        // Attach RequestInfo to the chunk if we have it
                        let chunk = StreamingChatResponse::from(chunk);
                        if let Some(info) = request_info.take() {
                            let chunk = DeltaWithRequestInfo {
                                data: chunk,
                                meta: info,
                            };
                            Some(sse::Event::default().json_data(chunk))
                        } else {
                            Some(sse::Event::default().json_data(chunk))
                        }
                    }
                    Ok(StreamingResponse::RequestInfo(_)) => {
                        // This should never happen since we already received it above.
                        debug_assert!(false, "got multiple RequestInfo");
                        None
                    }
                    Ok(StreamingResponse::ResponseInfo(_)) => {
                        // Need to figure out if there's some way we can send this along with the
                        // deltas as metadata, but it's difficult since the OpenAI format uses
                        // data-only SSE so we can't just define a new event type or something.
                        // Might work to send an extra chunk with an empty choices but need to see if
                        // that messes things up. Not a big deal though since the ResponseInfo
                        // doesn't contain much important, and it gets logged anyway.
                        None
                    }
                    Err(e) => {
                        let err = e.current_context();
                        let err_payload = if let Some(body) = &err.body {
                            // See if the error body looks like the OpenAI format, and if so just use it.
                            let message = &body["message"];
                            let error = &body["error"];

                            if let serde_json::Value::String(message) = message {
                                OpenAiSseError {
                                    error: Some(error.clone()),
                                    message: message.clone(),
                                }
                            } else {
                                OpenAiSseError {
                                    error: Some(body.clone()),
                                    message: err.to_string(),
                                }
                            }
                        } else {
                            OpenAiSseError {
                                error: None,
                                message: err.to_string(),
                            }
                        };

                        Some(sse::Event::default().event("error").json_data(err_payload))
                    }
                };

                // We're really just using `scan` to attach `request_info` as a persistent piece of
                // state. We don't actually want to end the stream, so wrap the value in Some so
                // `scan` won't end things. The `filter_map` in the next stage will filter out the
                // None values that come from the match statement.
                ready(Some(result))
            })
            .filter_map(|x| ready(x))
            .chain(futures::stream::once(ready(Ok(
                // Mimic OpenAI's [DONE] message
                sse::Event::default().data("[DONE]"),
            ))));

        Ok(Sse::new(stream).into_response())
    } else {
        let result = collect_response(result, n)
            .await
            .change_context(Error::Proxy)?;
        Ok(Json(ProxyRequestNonstreamingResult {
            response: result.response,
            meta: result.request_info,
        })
        .into_response())
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
        .route(
            "/",
            axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
        .route("/event", axum::routing::post(record_event))
        .route("/v1/event", axum::routing::post(record_event))
        .route("/chat", axum::routing::post(proxy_request))
        // We don't use the wildcard path, but allow calling with any path for compatibility with clients
        // that always append an API path to a base url.
        .route("/chat/*path", axum::routing::post(proxy_request))
        .route("/v1/chat/*path", axum::routing::post(proxy_request))
        .route("/healthz", axum::routing::get(|| async { "OK" }))
}
