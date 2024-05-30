use error_stack::Report;
use tracing::Span;

use crate::{
    database::logging::{CollectedProxiedResult, ProxyLogEntry},
    format::{
        ChatChoice, ChatMessage, SingleChatResponse, StreamingResponse, StreamingResponseInfo,
        StreamingResponseReceiver, StreamingResponseSender, UsageResponse,
    },
    providers::SingleProviderResponse,
    request::ProxiedResult,
    Error,
};

pub async fn handle_response(
    current_span: Span,
    log_entry: ProxyLogEntry,
    global_start: tokio::time::Instant,
    meta: ProxiedResult,
    chunk_rx: StreamingResponseReceiver,
    output_tx: StreamingResponseSender,
    log_tx: Option<&flume::Sender<ProxyLogEntry>>,
) {
    let response = collect_stream(
        current_span.clone(),
        log_entry,
        global_start,
        &meta,
        chunk_rx,
        output_tx,
        log_tx,
    )
    .await;
    let Ok((response, mut log_entry)) = response else {
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
            .body
            .choices
            .iter()
            .filter_map(|c| c.message.content.as_deref())
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
    current_span.record(
        "llm.completions.raw",
        serde_json::to_string(&response.body.choices).ok(),
    );
    current_span.record("llm.vendor", &meta.provider);
    current_span.record("llm.response.model", &response.body.model);
    current_span.record("llm.latency", this_send_time.as_millis());
    current_span.record("llm.retries", meta.num_retries);
    current_span.record("llm.rate_limited", meta.was_rate_limited);
    current_span.record("llm.usage.prompt_tokens", response.body.usage.prompt_tokens);
    current_span.record(
        "llm.finish_reason",
        response.body.choices.get(0).map(|c| &c.finish_reason),
    );
    current_span.record(
        "llm.usage.completion_tokens",
        response.body.usage.completion_tokens,
    );
    let total_tokens = response.body.usage.total_tokens.unwrap_or_else(|| {
        response.body.usage.prompt_tokens.unwrap_or(0)
            + response.body.usage.completion_tokens.unwrap_or(0)
    });
    current_span.record("llm.usage.total_tokens", total_tokens);

    if let Some(log_tx) = log_tx {
        log_entry.total_latency = Some(global_send_time);
        log_entry.num_retries = Some(meta.num_retries);
        log_entry.was_rate_limited = Some(meta.was_rate_limited);
        log_entry.response = Some(CollectedProxiedResult {
            body: response,
            provider: meta.provider,
            num_retries: meta.num_retries,
            was_rate_limited: meta.was_rate_limited,
        });

        log_tx.send_async(log_entry).await.ok();
    }
}

async fn collect_stream(
    current_span: Span,
    log_entry: ProxyLogEntry,
    global_start: tokio::time::Instant,
    meta: &ProxiedResult,
    chunk_rx: StreamingResponseReceiver,
    output_tx: StreamingResponseSender,
    log_tx: Option<&flume::Sender<ProxyLogEntry>>,
) -> Result<(SingleProviderResponse, ProxyLogEntry), ()> {
    let mut response = SingleChatResponse {
        created: 0,
        model: None,
        system_fingerprint: None,
        choices: Vec::new(),
        usage: UsageResponse {
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        },
    };

    let mut stats = StreamingResponseInfo {
        model: String::new(),
        meta: None,
    };

    while let Some(chunk) = chunk_rx.recv_async().await.ok() {
        match &chunk {
            Ok(StreamingResponse::Chunk(chunk)) => {
                if response.created == 0 {
                    response.created = chunk.created;
                }

                if response.model.is_none() {
                    response.model = chunk.model.clone();
                }

                if response.system_fingerprint.is_none() {
                    response.system_fingerprint = chunk.system_fingerprint.clone();
                }

                if !response.usage.is_empty() {
                    response.usage = chunk.usage.clone();
                }

                for choice in chunk.choices.iter() {
                    if choice.index >= response.choices.len() {
                        response.choices.resize_with(
                            std::cmp::max(chunk.choices.len(), choice.index + 1),
                            || ChatChoice {
                                index: 0,
                                message: ChatMessage {
                                    role: None,
                                    name: None,
                                    content: None,
                                    tool_calls: Vec::new(),
                                },
                                finish_reason: String::new(),
                            },
                        );

                        for i in 0..response.choices.len() {
                            response.choices[i].index = i;
                        }
                    }

                    let c = &mut response.choices[choice.index];
                    if c.message.role.is_none() {
                        c.message.role = choice.delta.role.clone();
                    }

                    if c.message.name.is_none() {
                        c.message.name = choice.delta.name.clone();
                    }

                    if c.message.content.is_none() {
                        match (&mut c.message.content, &choice.delta.content) {
                            (Some(content), Some(new_content)) => content.push_str(new_content),
                            (None, Some(new_content)) => {
                                c.message.content = Some(new_content.clone());
                            }
                            _ => {}
                        }
                    }

                    if let Some(finish) = choice.finish_reason.clone() {
                        c.finish_reason = finish;
                    }
                }
            }
            Ok(StreamingResponse::Finished(i)) => {
                stats = i.clone();
            }
            Ok(StreamingResponse::Single(res)) => {
                response = res.body.clone();
                stats = res.stats.clone();
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

    let output = SingleProviderResponse {
        body: response,
        stats,
    };

    Ok((output, log_entry))
}

pub async fn record_error(
    mut log_entry: ProxyLogEntry,
    error: &Report<Error>,
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
