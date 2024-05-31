use std::time::Duration;

use error_stack::{Report, ResultExt};
use eventsource_stream::{Event, Eventsource};
use futures::StreamExt;

use crate::{
    format::{ResponseInfo, StreamingChatResponse, StreamingResponse, StreamingResponseSender},
    providers::{ProviderError, ProviderErrorKind},
};

/// Stream an SSE response to the channel
///
/// `start_time` - the time the request was started
/// `response` - the response to stream
/// `chunk_tx` - the channel to send the chunks to
/// `map_chunk` - a function to map the event to a standard chat response.
///
/// `map_chunk` can return Ok(None) if the event should be skipped, as with Anthropic's
/// ping event.
pub async fn stream_sse_to_channel(
    response: reqwest::Response,
    chunk_tx: StreamingResponseSender,
    map_chunk: impl Fn(&Event) -> Result<Option<StreamingChatResponse>, Report<ProviderError>>
        + Send
        + Sync
        + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn(async move {
        let mut stream = response.bytes_stream().eventsource();
        let mut model: Option<String> = None;

        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    let chunk = map_chunk(&event);
                    tracing::trace!(chunk = ?chunk);
                    match chunk {
                        Ok(None) => continue,
                        Ok(Some(chunk)) => {
                            if model.is_none() {
                                model = chunk.model.clone();
                            }

                            let result = chunk_tx
                                .send_async(Ok(StreamingResponse::Chunk(chunk)))
                                .await;
                            if result.is_err() {
                                // Channel was closed
                                tracing::warn!("channel closed early");
                                return;
                            }
                        }
                        Err(e) => {
                            chunk_tx.send_async(Err(e)).await.ok();
                            return;
                        }
                    }
                }
                Err(e) => {
                    chunk_tx
                        .send_async(Err(e).change_context(ProviderError {
                            kind: ProviderErrorKind::ProviderClosedConnection,
                            status_code: None,
                            body: None,
                            latency: Duration::ZERO,
                        }))
                        .await
                        .ok();
                    return;
                }
            }
        }

        chunk_tx
            .send_async(Ok(StreamingResponse::ResponseInfo(ResponseInfo {
                meta: None,
                model: model.unwrap_or_default(),
            })))
            .await
            .ok();
    })
}
