//! Logging events to the database
use std::{borrow::Cow, time::Duration};

use chrono::Utc;
use tracing::instrument;
use uuid::Uuid;

use super::{Database, ProxyDatabase};
use crate::{format::ChatRequest, providers::SynchronousProviderResponse, ProxyRequestOptions};
pub struct ProxyLogEntry {
    pub id: Uuid,
    pub event_type: Cow<'static, str>,
    pub timestamp: chrono::DateTime<Utc>,
    pub request: Option<ChatRequest>,
    pub response: Option<CollectedProxiedResult>,
    pub total_latency: Option<Duration>,
    pub was_rate_limited: Option<bool>,
    pub num_retries: Option<u32>,
    pub error: Option<String>,
    pub options: ProxyRequestOptions,
}

pub struct CollectedProxiedResult {
    pub body: SynchronousProviderResponse,
    /// The provider which was used for the successful response.
    pub provider: String,
    pub num_retries: u32,
    pub was_rate_limited: bool,
}

pub fn start_database_logger(
    db: Database,
    batch_size: usize,
    debounce_time: Duration,
) -> (flume::Sender<ProxyLogEntry>, tokio::task::JoinHandle<()>) {
    let (log_tx, log_rx) = flume::unbounded();

    let task = tokio::task::spawn(database_logger_task(db, log_rx, batch_size, debounce_time));

    (log_tx, task)
}

async fn database_logger_task(
    db: Database,
    rx: flume::Receiver<ProxyLogEntry>,
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

                tracing::debug!("Received item");
                batch.push(item);

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

#[instrument(level = "trace", parent=None, skip(db, items), fields(chronicle.db_batch.num_items = items.len()))]
async fn write_batch(db: &dyn ProxyDatabase, items: Vec<ProxyLogEntry>) {
    let mut query = String::with_capacity(items.len() * 1024);

    query.push_str(
        "INSERT INTO chronicle_events
        (id, event_type, organization_id, project_id, user_id, chat_request, chat_response,
         error, provider, model, application, environment, request_organization_id, request_project_id,
         request_user_id, workflow_id, workflow_name, run_id, step, step_index,
         prompt_id, prompt_version,
         meta, response_meta, retries, rate_limited, request_latency_ms,
         total_latency_ms, created_at) VALUES\n",
    );

    const NUM_PARAMS: usize = 29;

    for i in 0..items.len() {
        if i > 0 {
            query.push_str(",\n");
        }

        let base_param = i * NUM_PARAMS + 1;
        query.push_str("($");
        query.push_str(&base_param.to_string());
        for param in (base_param + 1)..(base_param + NUM_PARAMS) {
            query.push_str(",$");
            query.push_str(&param.to_string());
        }
        query.push(')');
    }

    let result = db.write_log_batch(query, items).await;

    if let Err(e) = result {
        tracing::error!(error = ?e, "Failed to write logs to database");
    }
}
