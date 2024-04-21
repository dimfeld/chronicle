use std::{fmt::Write, time::Duration};

use chrono::Utc;
use error_stack::Report;
use serde::Serialize;

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

// Start a task
// add batches of data to the database
// dump the data periodically
//

pub struct ProxyLogEntry {
    pub timestamp: chrono::DateTime<Utc>,
    pub request: ChatRequest,
    pub response: Option<ProviderResponse>,
    pub error: Option<Report<Error>>,
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

    // This also needs to have some timout on how long it will wait before sending a batch
    // to Postgres. Probably something low like 5 seconds. Also make this configurable, it
    // should be higher when running as a server compared to running in a local script.

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
    // Create INSERT statement and push it
    let mut query = String::with_capacity(items.len() * 1024);

    query.push_str(
        "INSERT INTO chronicle_proxy_log 
        (id, organization_id, project_id, user_id, chat_request, chat_response,
         error, application, environment, request_organization_id, request_project_id,
         request_user_id, workflow_id, workflow_name, run_id, step, step_index,
         extra_meta, response_meta, retries, rate_limited, request_latency_ms,
         total_latency_ms, created_at) VALUES\n",
    );

    for item in items {
        let id = uuid::Uuid::now_v7();
        query.push('\'');
        write!(query, "{}", id).unwrap();
        query.push('\'');

        option_field(&mut query, item.options.internal_metadata.organization_id);
        option_field(&mut query, item.options.internal_metadata.project_id);
        option_field(&mut query, item.options.internal_metadata.user_id);

        json_field(&mut query, item.request);
        json_field(&mut query, item.response);
        option_field(&mut query, item.error);

        option_field(&mut query, item.request.model);

        option_field(&mut query, item.options.metadata.application);
        option_field(&mut query, item.options.metadata.environment);
        option_field(&mut query, item.options.metadata.organization_id);
        option_field(&mut query, item.options.metadata.project_id);
        option_field(&mut query, item.options.metadata.user_id);
        option_field(&mut query, item.options.metadata.workflow_id);
        option_field(&mut query, item.options.metadata.workflow_name);
        option_field(&mut query, item.options.metadata.run_id);
        option_field(&mut query, item.options.metadata.step);
        option_field(&mut query, item.options.metadata.step_index);
        option_field(&mut query, item.options.metadata.extra_meta);
        option_field(&mut query, item.options.metadata.response_meta);
        option_field(&mut query, item.response.retries);
        option_field(&mut query, item.options.metadata.rate_limited);
        option_field(&mut query, item.options.metadata.request_latency_ms);
        option_field(&mut query, item.options.metadata.total_latency_ms);
        option_field(&mut query, item.options.metadata.created_at);
    }
}

fn option_field(query: &mut String, f: Option<String>) {
    if let Some(f) = f {
        query.push('\'');

        for c in f.chars() {
            if c == '\'' {
                query.push_str("''");
            } else {
                query.push(c);
            }
        }

        query.push('\'');
    } else {
        query.push_str(", NULL");
    }
}

fn json_field(query: &mut String, data: impl Serialize) {
    todo!()
}

#[cfg(feature = "migrations")]
pub mod migrations {
    use super::Pool;

    #[cfg(feature = "sqlite")]
    const SQLITE_MIGRATIONS: &[&'static str] = &[include_str!(
        "../migrations/20140419_chronicle_proxy_init_sqlite.sql"
    )];

    #[cfg(feature = "sqlite")]
    pub async fn init_sqlite_db(pool: &Pool) -> Result<(), sqlx::Error> {
        run_migrations(pool, SQLITE_MIGRATIONS).await?;
    }

    #[cfg(feature = "postgres")]
    const POSTGRESQL_MIGRATIONS: &[&'static str] = &[include_str!(
        "../migrations/20240419_chronicle_proxy_init_postgresql.sql"
    )];

    #[cfg(feature = "postgres")]
    pub async fn init_postgresql_db(pool: &Pool) -> Result<(), sqlx::Error> {
        run_migrations(pool, POSTGRESQL_MIGRATIONS).await
    }

    #[cfg(not(all(feature = "sqlite", feature = "postgres")))]
    pub async fn init_db(pool: &Pool) -> Result<(), sqlx::Error> {
        #[cfg(feature = "sqlite")]
        init_sqlite_db(pool).await?;

        #[cfg(feature = "postgres")]
        init_postgresql_db(pool).await?;

        Ok(())
    }

    pub async fn run_migrations(pool: &Pool, migrations: &[&str]) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        let migration_version = sqlx::query_scalar::<_, i32>(
            "SELECT value::int FROM chronicle_meta WHERE key='migration_version'",
        )
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .unwrap_or(0) as usize;

        let start_migration = migration_version.min(migrations.len());
        for migration in &migrations[start_migration..] {
            sqlx::query(migration).execute(&mut *tx).await?;
        }

        let new_version = migrations.len();

        sqlx::query(
            "UPDATE chronicle_meta
            SET value=$1::jsonb WHERE key='migration_version",
        )
        .bind(new_version as i32)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
