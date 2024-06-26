//! Logging events to the database
use std::{borrow::Cow, time::Duration};

use chrono::Utc;
use smallvec::SmallVec;
use tracing::instrument;
use uuid::Uuid;

use super::{Database, ProxyDatabase};
use crate::{
    format::{ChatRequest, ResponseInfo, SingleChatResponse},
    workflow_events::{EventPayload, WorkflowEvent},
    ProxyRequestOptions,
};

/// An event from the proxy.
#[derive(Debug)]
pub struct ProxyLogEvent {
    /// A unique ID for this event
    pub id: Uuid,
    /// The type of event
    pub event_type: Cow<'static, str>,
    /// The timestamp of the event
    pub timestamp: chrono::DateTime<Utc>,
    /// The request that was proxied
    pub request: Option<ChatRequest>,
    /// The response from the model provider
    pub response: Option<CollectedProxiedResult>,
    /// The latency of the request that succeeded
    pub latency: Option<Duration>,
    /// The total latency of the request, including retries.
    pub total_latency: Option<Duration>,
    /// Whether the request was rate limited
    pub was_rate_limited: Option<bool>,
    /// The number of retries
    pub num_retries: Option<u32>,
    /// The error that occurred, if any.
    pub error: Option<serde_json::Value>,
    /// The options that were used for the request
    pub options: ProxyRequestOptions,
}

impl ProxyLogEvent {
    /// Create a new event from a submitted payload
    pub fn from_payload(id: Uuid, payload: EventPayload) -> Self {
        let extra = match payload.data {
            Some(serde_json::Value::Object(m)) => Some(m),
            _ => None,
        };

        ProxyLogEvent {
            id,
            event_type: Cow::Owned(payload.typ),
            timestamp: payload.time.unwrap_or_else(|| Utc::now()),
            request: None,
            response: None,
            total_latency: None,
            latency: None,
            was_rate_limited: None,
            num_retries: None,
            error: payload.error,
            options: ProxyRequestOptions {
                metadata: crate::ProxyRequestMetadata {
                    extra,
                    step_id: Some(payload.step_id),
                    run_id: Some(payload.run_id),
                    ..Default::default()
                },
                internal_metadata: payload.internal_metadata.unwrap_or_default(),
                ..Default::default()
            },
        }
    }
}

/// A response from the model provider, collected into a single body if it was streamed
#[derive(Debug)]
pub struct CollectedProxiedResult {
    /// The response itself
    pub body: SingleChatResponse,
    /// Other information about the response
    pub info: ResponseInfo,
    /// The provider which was used for the successful response.
    pub provider: String,
}

/// An event to be logged
#[derive(Debug)]
pub enum ProxyLogEntry {
    /// The result of a proxied model request
    Proxied(Box<ProxyLogEvent>),
    /// An update from a workflow step or run
    Workflow(WorkflowEvent),
}

/// A channel on which log events can be sent.
pub type LogSender = flume::Sender<SmallVec<[ProxyLogEntry; 1]>>;

/// Start the database logger task
pub fn start_database_logger(
    db: Database,
    batch_size: usize,
    debounce_time: Duration,
) -> (LogSender, tokio::task::JoinHandle<()>) {
    let (log_tx, log_rx) = flume::unbounded();

    let task = tokio::task::spawn(database_logger_task(db, log_rx, batch_size, debounce_time));

    (log_tx, task)
}

async fn database_logger_task(
    db: Database,
    rx: flume::Receiver<SmallVec<[ProxyLogEntry; 1]>>,
    batch_size: usize,
    debounce_time: Duration,
) {
    let mut batch = Vec::with_capacity(batch_size);

    loop {
        tokio::select! {
            item = rx.recv_async() => {
                let Ok(item) = item else {
                    // channel closed so we're done
                    break;
                };

                tracing::debug!(num_items=item.len(), "Received items");
                batch.extend(item);

                if batch.len() >= batch_size {
                    let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));
                    write_batch(db.as_ref(), send_batch).await;
                }

            }
            _ = tokio::time::sleep(debounce_time), if !batch.is_empty() => {
                let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));
                write_batch(db.as_ref(), send_batch).await;
            }
        }
    }
    tracing::debug!("Closing database logger");

    if !batch.is_empty() {
        write_batch(db.as_ref(), batch).await;
    }
}

pub(super) const EVENT_INSERT_PREFIX: &str =
        "INSERT INTO chronicle_events
        (id, event_type, organization_id, project_id, user_id, chat_request, chat_response,
         error, provider, model, application, environment, request_organization_id, request_project_id,
         request_user_id, workflow_id, workflow_name, run_id, step_id, step_index,
         prompt_id, prompt_version,
         meta, response_meta, retries, rate_limited, request_latency_ms,
         total_latency_ms, created_at) VALUES\n";

#[instrument(level = "trace", parent=None, skip(db, items), fields(chronicle.db_batch.num_items = items.len()))]
async fn write_batch(db: &dyn ProxyDatabase, items: Vec<ProxyLogEntry>) {
    let result = db.write_log_batch(items).await;

    if let Err(e) = result {
        tracing::error!(error = ?e, "Failed to write logs to database");
    }
}
