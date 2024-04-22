mod any_layer;
#[cfg(feature = "migrations")]
pub mod migrations;

use std::{
    fmt::{Display, Write},
    time::Duration,
};

use chrono::Utc;
use error_stack::Report;
use serde::Serialize;

use self::any_layer::DbAbstraction;
use crate::{
    format::{ChatRequest, ChatResponse},
    providers::ProviderResponse,
    Error, ProxyRequestOptions,
};

#[cfg(feature = "any-db")]
pub type Database = sqlx::Any;

#[cfg(all(not(feature = "any-db"), feature = "postgres"))]
pub type Database = sqlx::Postgres;

#[cfg(all(not(feature = "any-db"), feature = "sqlite"))]
pub type Database = sqlx::Sqlite;

pub type Pool = sqlx::Pool<Database>;

pub struct ProxyLogEntry {
    pub timestamp: chrono::DateTime<Utc>,
    pub request: ChatRequest,
    pub response: Option<ProviderResponse>,
    pub total_latency: Duration,
    pub error: Option<Report<Error>>,
    pub options: ProxyRequestOptions,
}

pub fn start_database_logger(
    pool: Pool,
    batch_size: usize,
    debounce_time: Duration,
) -> (flume::Sender<ProxyLogEntry>, tokio::task::JoinHandle<()>) {
    let (log_tx, log_rx) = flume::unbounded();

    #[cfg(feature = "any-db")]
    let db_type = DbAbstraction::from_url(&pool.connect_options().database_url);
    #[cfg(all(not(feature = "any-db"), feature = "postgres"))]
    let db_type = DbAbstraction::Postgres;
    #[cfg(all(not(feature = "any-db"), feature = "sqlite"))]
    let db_type = DbAbstraction::Sqlite;

    let task = tokio::task::spawn(database_logger_task(
        pool,
        log_rx,
        batch_size,
        debounce_time,
        db_type,
    ));

    (log_tx, task)
}

async fn database_logger_task(
    pool: Pool,
    rx: flume::Receiver<ProxyLogEntry>,
    batch_size: usize,
    debounce_time: Duration,
    db_type: DbAbstraction,
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
                    write_batch(&pool, db_type, send_batch).await;
                }

            }
            _ = tokio::time::sleep(debounce_time), if !batch.is_empty() => {
                let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));
                write_batch(&pool, db_type, send_batch).await;
            }
        }
    }

    if !batch.is_empty() {
        write_batch(&pool, db_type, batch).await;
    }
}

async fn write_batch(pool: &Pool, db_type: DbAbstraction, items: Vec<ProxyLogEntry>) {
    let mut query = String::with_capacity(items.len() * 1024);

    query.push_str(
        "INSERT INTO chronicle_events
        (id, organization_id, project_id, user_id, chat_request, chat_response,
         error, application, environment, request_organization_id, request_project_id,
         request_user_id, workflow_id, workflow_name, run_id, step, step_index,
         extra_meta, response_meta, retries, rate_limited, request_latency_ms,
         total_latency_ms, created_at) VALUES\n",
    );

    for (i, item) in items.into_iter().enumerate() {
        let id = uuid::Uuid::now_v7();
        let organization_id = EscapedNullable(item.options.internal_metadata.organization_id);
        let project_id = EscapedNullable(item.options.internal_metadata.project_id);
        let user_id = EscapedNullable(item.options.internal_metadata.user_id);

        let chat_request = Escaped(serde_json::to_string(&item.request).unwrap_or_default());
        let chat_response = EscapedNullable(
            item.response
                .as_ref()
                .and_then(|r| serde_json::to_string(&r.body).ok()),
        );
        let error = EscapedNullable(item.error.map(|e| format!("{:?}", e)));
        let model = Escaped(item.request.model.unwrap_or_default());
        let application = EscapedNullable(item.options.metadata.application);
        let environment = EscapedNullable(item.options.metadata.environment);
        let request_organization_id = EscapedNullable(item.options.metadata.organization_id);
        let request_project_id = EscapedNullable(item.options.metadata.project_id);
        let request_user_id = EscapedNullable(item.options.metadata.user_id);
        let workflow_id = EscapedNullable(item.options.metadata.workflow_id);
        let workflow_name = EscapedNullable(item.options.metadata.workflow_name);
        let run_id = EscapedNullable(item.options.metadata.run_id);
        let step = EscapedNullable(item.options.metadata.step);
        let step_index = Nullable(item.options.metadata.step_index);
        let extra_meta = EscapedNullable(
            item.options
                .metadata
                .extra
                .and_then(|m| serde_json::to_string(&m).ok()),
        );
        let response_meta = EscapedNullable(
            item.response
                .as_ref()
                .and_then(|r| serde_json::to_string(&r.body).ok()),
        );
        let retries = Nullable(item.response.as_ref().map(|r| r.retries));
        let rate_limited = nullable_bool(item.response.as_ref().map(|r| r.rate_limited));
        let request_latency_ms = Nullable(item.response.map(|r| r.latency.as_millis() as u64));
        let total_latency_ms = item.total_latency.as_millis() as u64;
        let created_at = db_type.timestamp_value(&item.timestamp);

        if i > 0 {
            query.push_str(",\n");
        }

        write!(
            query,
            "(
                {id}, {organization_id}, {project_id}, {user_id},
                {chat_request}, {chat_response}, {error},
                {model}, {application}, {environment},
                {request_organization_id}, {request_project_id}, {request_user_id},
                {workflow_id}, {workflow_name}, {run_id}, {step}, {step_index},
                {extra_meta}, {response_meta}, {retries}, {rate_limited},
                {request_latency_ms}, {total_latency_ms}, {created_at}
            )"
        )
        .ok();
    }

    let result = sqlx::query(&query).execute(pool).await;

    if let Err(e) = result {
        tracing::error!(error = ?e, "Failed to write logs to database");
    }
}

struct Escaped<T: AsRef<str>>(T);

impl<T: AsRef<str>> std::fmt::Display for Escaped<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('\'')?;

        let mut segments = self.0.as_ref().split('\'');
        if let Some(c) = segments.next() {
            f.write_str(c)?;
        }
        for c in segments {
            f.write_char('\'')?;
            f.write_char('\'')?;
            f.write_str(c)?;
        }

        f.write_char('\'')?;
        Ok(())
    }
}

struct EscapedNullable<T: AsRef<str>>(Option<T>);

impl<T: AsRef<str>> std::fmt::Display for EscapedNullable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.0.as_ref() {
            Escaped(value).fmt(f)
        } else {
            f.write_str("NULL")
        }
    }
}

// The UpperExp bound is an easy way to ensure that you can only pass in a number.
struct Nullable<T: Display + std::fmt::UpperExp>(Option<T>);

impl<T: Display + std::fmt::UpperExp> std::fmt::Display for Nullable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = self.0.as_ref() {
            std::fmt::Display::fmt(value, f)
        } else {
            f.write_str("NULL")
        }
    }
}

fn nullable_bool(value: Option<bool>) -> &'static str {
    value
        .map(|b| if b { "true" } else { "false" })
        .unwrap_or("NULL")
}

#[cfg(test)]
mod test {
    use crate::database::{Escaped, EscapedNullable, Nullable};

    #[test]
    fn escaped() {
        assert_eq!(Escaped("foo").to_string(), "'foo'");
        assert_eq!(Escaped("foo'bar").to_string(), "'foo''bar'");
        assert_eq!(Escaped("foo''bar").to_string(), "'foo''''bar'");
        assert_eq!(Escaped("foo''bar'").to_string(), "'foo''''bar'''");
        assert_eq!(Escaped("foobar'").to_string(), "'foobar'''");
        assert_eq!(Escaped("foobar''").to_string(), "'foobar'''''");
        assert_eq!(Escaped("'foobar").to_string(), "'''foobar'");

        assert_eq!(EscapedNullable::<String>(None).to_string(), "NULL");
        assert_eq!(EscapedNullable(Some("foo")).to_string(), "'foo'");
        assert_eq!(EscapedNullable(Some("foo'bar")).to_string(), "'foo''bar'");
    }

    #[test]
    fn nullable() {
        assert_eq!(Nullable::<i32>(None).to_string(), "NULL");
        assert_eq!(Nullable::<i32>(Some(3)).to_string(), "3");
    }

    #[test]
    fn nullable_bool() {
        assert_eq!(super::nullable_bool(None), "NULL");
        assert_eq!(super::nullable_bool(Some(true)), "true");
        assert_eq!(super::nullable_bool(Some(false)), "false");
    }
}
