use std::time::Duration;

use chrono::Utc;
use error_stack::Report;

use crate::{
    format::{ChatRequest, ChatResponse},
    ProxyRequestOptions,
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

pub enum ProxyLogEntry {
    Start(ProxyLogStart),
    Finish(ProxyLogFinish),
    Error(ProxyLogError),
}

pub struct ProxyLogStart {
    pub timestamp: chrono::DateTime<Utc>,
    pub message: ChatRequest,
    pub options: ProxyRequestOptions,
}

pub struct ProxyLogFinish {
    pub timestamp: chrono::DateTime<Utc>,
    pub response: ChatResponse,
}

pub struct ProxyLogError {
    pub timestamp: chrono::DateTime<Utc>,
    pub error: Report<ProxyLogError>,
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

    while let Ok(log_item) = rx.recv_async().await {
        batch.push(log_item);
        if batch.len() >= batch_size {
            let send_batch = std::mem::replace(&mut batch, Vec::with_capacity(batch_size));

            // todo send the batch to the writer task
        }
    }
}

#[cfg(feature = "migrations")]
mod migrations {
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
        "../migrations/20140419_chronicle_proxy_init_postgresql.sql"
    )];

    #[cfg(feature = "postgres")]
    pub async fn init_postgresql_db(pool: Pool) {
        run_migrations(pool, POSTGRESQL_MIGRATIONS).await?;
    }

    #[cfg(not(all(feature = "sqlite", feature = "postgres")))]
    pub async fn init_db(pool: &Pool) -> Result<(), sqlx::Error> {
        #[cfg(feature = "sqlite")]
        init_sqlite_db(pool).await?;

        #[cfg(feature = "postgres")]
        init_postgresql_db(pool).await?;

        Ok(())
    }

    async fn run_migrations(pool: &Pool, migrations: &[&str]) -> Result<(), sqlx::Error> {
        let tx = pool.begin().await?;
        let migration_version = sqlx::query_scalar(
            "SELECT value::int FROM chronicle_meta WHERE key='migration_version'",
        )
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .unwrap_or(0);

        let start_migration = migration_version.min(migrations.len());
        for migration in &migrations[start_migration..] {
            sqlx::query(migration).execute(&mut *tx).await?;
        }

        let new_version = migrations.len();

        sqlx::query(
            "UPDATE chronicle_meta
            SET value=$1 WHERE key='migration_version",
        )
        .bind(new_version)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }
}
