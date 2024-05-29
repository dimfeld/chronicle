use std::str::FromStr;

use chronicle_proxy::database::Database;
use error_stack::Report;
use sqlx::{sqlite::SqliteConnectOptions, PgPool, SqlitePool};

pub async fn init_database(db: Option<String>) -> Result<Option<Database>, Report<sqlx::Error>> {
    let Some(db) = db else {
        tracing::info!("No database configured");
        return Ok(None);
    };

    let pg = db.starts_with("postgresql://") || db.starts_with("postgres://");

    if pg {
        // Print the connection string without the password
        let ops = sqlx::postgres::PgConnectOptions::from_str(&db)?;
        let connection_string = reconstruct_pg_connstr(&ops);
        tracing::info!("Connecting to PostgreSQL database at {connection_string}");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_with(ops)
            .await?;
        Ok(Some(init_pg(pool).await?))
    } else {
        tracing::info!("Opening SQLite database at {db}");

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::from_str(&db)?
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                    .create_if_missing(true),
            )
            .await?;

        Ok(Some(init_sqlite(pool).await?))
    }
}

pub(crate) async fn init_pg(pool: PgPool) -> Result<Database, Report<sqlx::Error>> {
    chronicle_proxy::database::postgres::run_default_migrations(&pool).await?;

    Ok(chronicle_proxy::database::postgres::PostgresDatabase::new(
        pool,
    ))
}

pub(crate) async fn init_sqlite(pool: SqlitePool) -> Result<Database, Report<sqlx::Error>> {
    chronicle_proxy::database::sqlite::run_default_migrations(&pool).await?;

    Ok(chronicle_proxy::database::sqlite::SqliteDatabase::new(pool))
}

fn reconstruct_pg_connstr(ops: &sqlx::postgres::PgConnectOptions) -> String {
    let mut c = String::from("postgresql://");
    let user = ops.get_username();
    let host = ops.get_host();
    let port = ops.get_port();
    let db = ops.get_database();

    if !user.is_empty() {
        c.push_str(user);
        c.push('@');
    }

    if !host.is_empty() {
        c.push_str(host);
    }

    if port > 0 {
        c.push(':');
        c.push_str(&port.to_string());
    }

    if let Some(db) = db {
        c.push('/');
        c.push_str(db);
    }

    c
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::FutureExt;
    use serde_json::json;
    use temp_dir::TempDir;

    use super::*;
    use crate::{
        config::{Configs, LocalServerConfig},
        serve,
    };

    #[sqlx::test]
    async fn test_postgres(pool: PgPool) {
        filigree::tracing_config::test::init();
        let db = init_pg(pool.clone()).await.expect("Creating database");
        test_proxy(10034, db).await;
        let events = sqlx::query("SELECT * FROM chronicle_events")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn sqlite_with_path() {
        filigree::tracing_config::test::init();
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(db_path.to_string_lossy().as_ref())
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();
        let db = init_sqlite(pool.clone()).await.unwrap();
        test_proxy(10035, db).await;
        let events = sqlx::query("SELECT * FROM chronicle_events")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn sqlite_with_url() {
        filigree::tracing_config::test::init();
        let dir = TempDir::new().unwrap();
        let db_path = format!("sqlite://{}", dir.path().join("test.db").display());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(&db_path)
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();
        let db = init_sqlite(pool.clone()).await.unwrap();
        test_proxy(10036, db).await;
        let events = sqlx::query("SELECT * FROM chronicle_events")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
    }

    async fn test_proxy(port: u16, db: Database) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let shutdown_rx = shutdown_rx.then(|_| async { () });

        let server = tokio::spawn(async move {
            serve(
                LocalServerConfig {
                    database: None,
                    port: Some(port),
                    dotenv: Some(false),
                    host: None,
                },
                Configs {
                    global: vec![],
                    cwd: vec![],
                },
                Some(db),
                shutdown_rx,
            )
            .await
        });

        let client = reqwest::Client::new();

        let mut tries = 100;

        while tries > 0 {
            if client
                .get(format!("http://localhost:{port}/healthz"))
                .send()
                .await
                .is_ok()
            {
                break;
            }
            tries -= 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let base_url = format!("http://localhost:{port}");
        client
            .post(&format!("{base_url}/event"))
            .json(&json!({
                "type": "an_event",
                "data": {
                    "test": true
                },
                "metadata": {
                    "application": "abc"
                }
            }))
            .send()
            .await
            .unwrap();

        drop(shutdown_tx);
        server
            .await
            .expect("server panicked")
            .expect("server returned error");
    }
}
