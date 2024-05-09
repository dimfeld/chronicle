//! Logging events to the database
use std::{borrow::Cow, time::Duration};

use chrono::Utc;
use uuid::Uuid;

use super::Pool;
use crate::{format::ChatRequest, request::ProxiedResult, ProxyRequestOptions};
pub struct ProxyLogEntry {
    pub id: Uuid,
    pub event_type: Cow<'static, str>,
    pub timestamp: chrono::DateTime<Utc>,
    pub request: Option<ChatRequest>,
    pub response: Option<ProxiedResult>,
    pub total_latency: Option<Duration>,
    pub was_rate_limited: Option<bool>,
    pub num_retries: Option<u32>,
    pub error: Option<String>,
    pub options: ProxyRequestOptions,
}

pub fn start_database_logger(
    pool: Pool,
    batch_size: usize,
    debounce_time: Duration,
) -> (flume::Sender<ProxyLogEntry>, tokio::task::JoinHandle<()>) {
    let (log_tx, log_rx) = flume::unbounded();

    let task = tokio::task::spawn(database_logger_task(
        pool,
        log_rx,
        batch_size,
        debounce_time,
    ));

    (log_tx, task)
}

async fn database_logger_task(
    pool: Pool,
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

                batch.push(item);

                if batch.len() >= batch_size {
                    let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));
                    write_batch(&pool, send_batch).await;
                }

            }
            _ = tokio::time::sleep(debounce_time), if !batch.is_empty() => {
                let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));
                write_batch(&pool, send_batch).await;
            }
        }
    }

    if !batch.is_empty() {
        write_batch(&pool, batch).await;
    }
}

async fn write_batch(pool: &Pool, items: Vec<ProxyLogEntry>) {
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

    let mut query = sqlx::query(&query);

    for item in items.into_iter() {
        let (rmodel, rprovider, rbody, rmeta, rlatency) = match item.response.map(|r| {
            (
                r.body.model.clone(),
                r.provider,
                r.body,
                r.meta,
                r.latency.as_millis() as i64,
            )
        }) {
            Some((rmodel, rprovider, rbody, rmeta, rlatency)) => {
                (rmodel, Some(rprovider), Some(rbody), rmeta, Some(rlatency))
            }
            None => (None, None, None, None, None),
        };

        let model = rmodel
            .or_else(|| item.request.as_ref().and_then(|r| r.model.clone()))
            .unwrap_or_default();

        let extra = item.options.metadata.extra.filter(|m| !m.is_empty());

        if cfg!(feature = "sqlite") {
            // sqlx encodes UUIDs as binary blobs by default with Sqlite, which is often nice
            // but not what we want here.
            query = query.bind(item.id.to_string());
        } else {
            query = query.bind(item.id);
        }

        query = query
            .bind(item.event_type)
            .bind(item.options.internal_metadata.organization_id)
            .bind(item.options.internal_metadata.project_id)
            .bind(item.options.internal_metadata.user_id)
            .bind(sqlx::types::Json(item.request))
            .bind(sqlx::types::Json(rbody))
            .bind(sqlx::types::Json(item.error))
            .bind(rprovider)
            .bind(model)
            .bind(item.options.metadata.application)
            .bind(item.options.metadata.environment)
            .bind(item.options.metadata.organization_id)
            .bind(item.options.metadata.project_id)
            .bind(item.options.metadata.user_id)
            .bind(item.options.metadata.workflow_id)
            .bind(item.options.metadata.workflow_name)
            .bind(item.options.metadata.run_id)
            .bind(item.options.metadata.step)
            .bind(item.options.metadata.step_index.map(|i| i as i32))
            .bind(item.options.metadata.prompt_id)
            .bind(item.options.metadata.prompt_version.map(|i| i as i32))
            .bind(sqlx::types::Json(extra))
            .bind(rmeta)
            .bind(item.num_retries.map(|n| n as i32))
            .bind(item.was_rate_limited)
            .bind(rlatency)
            .bind(item.total_latency.map(|d| d.as_millis() as i64))
            .bind(item.timestamp);
    }

    let result = query.execute(pool).await;

    if let Err(e) = result {
        tracing::error!(error = ?e, "Failed to write logs to database");
    }
}
