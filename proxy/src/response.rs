use error_stack::{Report, ResultExt};
use serde::Serialize;
use tracing::Span;

use crate::{
    database::logging::{CollectedProxiedResult, ProxyLogEntry},
    format::{
        RequestInfo, ResponseInfo, SingleChatResponse, StreamingResponse,
        StreamingResponseReceiver, StreamingResponseSender,
    },
    request::TryModelChoicesResult,
    Error,
};

pub async fn handle_response(
    current_span: Span,
    log_entry: ProxyLogEntry,
    global_start: tokio::time::Instant,
    request_n: usize,
    meta: TryModelChoicesResult,
    chunk_rx: StreamingResponseReceiver,
    output_tx: StreamingResponseSender,
    log_tx: Option<&flume::Sender<ProxyLogEntry>>,
) {
    let response = collect_stream(
        current_span.clone(),
        log_entry,
        global_start,
        request_n,
        &meta,
        chunk_rx,
        output_tx,
        log_tx,
    )
    .await;
    let Ok((response, info, mut log_entry)) = response else {
        // Errors were already handled by collect_stream.
        return;
    };
    let global_send_time = global_start.elapsed();
    let this_send_time = meta.start_time.elapsed();
    log_entry.latency = Some(this_send_time);

    // In case of retries, this might be meaningfully different from the main latency.
    current_span.record("llm.total_latency", global_send_time.as_millis());

    current_span.record(
        "llm.completions",
        response
            .choices
            .iter()
            .filter_map(|c| c.message.content.as_deref())
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
    current_span.record(
        "llm.completions.raw",
        serde_json::to_string(&response.choices).ok(),
    );
    current_span.record("llm.vendor", &meta.provider);
    current_span.record("llm.response.model", &response.model);
    current_span.record("llm.latency", this_send_time.as_millis());
    current_span.record("llm.retries", meta.num_retries);
    current_span.record("llm.rate_limited", meta.was_rate_limited);
    current_span.record("llm.usage.prompt_tokens", response.usage.prompt_tokens);
    current_span.record(
        "llm.finish_reason",
        response.choices.get(0).map(|c| &c.finish_reason),
    );
    current_span.record(
        "llm.usage.completion_tokens",
        response.usage.completion_tokens,
    );
    let total_tokens = response.usage.total_tokens.unwrap_or_else(|| {
        response.usage.prompt_tokens.unwrap_or(0) + response.usage.completion_tokens.unwrap_or(0)
    });
    current_span.record("llm.usage.total_tokens", total_tokens);

    if let Some(log_tx) = log_tx {
        log_entry.total_latency = Some(global_send_time);
        log_entry.num_retries = Some(meta.num_retries);
        log_entry.was_rate_limited = Some(meta.was_rate_limited);
        log_entry.response = Some(CollectedProxiedResult {
            body: response,
            info,
            provider: meta.provider,
        });

        log_tx.send_async(log_entry).await.ok();
    }
}

/// Internal stream collection that saves the information for logging.
async fn collect_stream(
    current_span: Span,
    log_entry: ProxyLogEntry,
    global_start: tokio::time::Instant,
    request_n: usize,
    meta: &TryModelChoicesResult,
    chunk_rx: StreamingResponseReceiver,
    output_tx: StreamingResponseSender,
    log_tx: Option<&flume::Sender<ProxyLogEntry>>,
) -> Result<(SingleChatResponse, ResponseInfo, ProxyLogEntry), ()> {
    let mut response = SingleChatResponse::new_for_collection(request_n);

    let mut res_stats = ResponseInfo {
        model: String::new(),
        meta: None,
    };

    // Collect the message chunks so we can log the result, while also passing them on to the output channel.
    while let Some(chunk) = chunk_rx.recv_async().await.ok() {
        match &chunk {
            Ok(StreamingResponse::Chunk(chunk)) => {
                response.merge_delta(chunk);
            }
            Ok(StreamingResponse::ResponseInfo(i)) => {
                res_stats = i.clone();
            }
            Ok(StreamingResponse::RequestInfo(_)) => {
                // Don't need to handle RequestInfo since we've already incorporated its
                // information into `log_entry`.
            }
            Ok(StreamingResponse::Single(res)) => {
                response = res.clone();
            }
            Err(e) => {
                record_error(
                    log_entry,
                    e,
                    global_start,
                    meta.num_retries,
                    meta.was_rate_limited,
                    current_span,
                    log_tx,
                )
                .await;
                output_tx.send_async(chunk).await.ok();
                return Err(());
            }
        }

        output_tx.send_async(chunk).await.ok();
    }

    Ok((response, res_stats, log_entry))
}

pub async fn record_error<E: std::fmt::Debug + std::fmt::Display>(
    mut log_entry: ProxyLogEntry,
    error: E,
    send_start: tokio::time::Instant,
    num_retries: u32,
    was_rate_limited: bool,
    current_span: Span,
    log_tx: Option<&flume::Sender<ProxyLogEntry>>,
) {
    tracing::error!(error.full=?error, "Request failed");

    current_span.record("error", error.to_string());
    current_span.record("llm.retries", num_retries);
    current_span.record("llm.rate_limited", was_rate_limited);

    if let Some(log_tx) = log_tx {
        log_entry.total_latency = Some(send_start.elapsed());
        log_entry.num_retries = Some(num_retries);
        log_entry.was_rate_limited = Some(was_rate_limited);
        log_entry.error = Some(format!("{:?}", error));
        log_tx.send_async(log_entry).await.ok();
    }
}

#[derive(Serialize, Debug)]
pub struct CollectedResponse {
    pub request_info: RequestInfo,
    pub response_info: ResponseInfo,
    pub was_streaming: bool,
    pub num_chunks: usize,
    pub response: SingleChatResponse,
}

/// Collect a stream contents into a single response
pub async fn collect_response(
    receiver: StreamingResponseReceiver,
    request_n: usize,
) -> Result<CollectedResponse, Report<Error>> {
    let mut request_info = None;
    let mut response_info = None;
    let mut was_streaming = false;

    let mut num_chunks = 0;
    let mut response = SingleChatResponse::new_for_collection(request_n);

    while let Ok(res) = receiver.recv_async().await {
        tracing::debug!(?res, "Got response chunk");
        match res.change_context(Error::ModelError)? {
            StreamingResponse::RequestInfo(info) => {
                debug_assert!(request_info.is_none(), "Saw multiple RequestInfo objects");
                debug_assert_eq!(num_chunks, 0, "RequestInfo was not the first chunk");
                request_info = Some(info);
            }
            StreamingResponse::ResponseInfo(info) => {
                debug_assert!(response_info.is_none(), "Saw multiple ResponseInfo objects");
                response_info = Some(info);
            }
            StreamingResponse::Single(res) => {
                debug_assert_eq!(num_chunks, 0, "Saw more than one non-streaming chunk");
                num_chunks += 1;
                response = res;
            }
            StreamingResponse::Chunk(res) => {
                was_streaming = true;
                num_chunks += 1;
                response.merge_delta(&res);
            }
        }
    }

    let request_info = request_info.ok_or(Error::MissingStreamInformation("request info"))?;
    Ok(CollectedResponse {
        response_info: response_info.unwrap_or_else(|| ResponseInfo {
            meta: None,
            model: request_info.model.clone(),
        }),
        request_info,
        was_streaming,
        num_chunks,
        response,
    })
}
