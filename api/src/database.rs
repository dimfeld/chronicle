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

    use chrono::{DateTime, TimeZone, Utc};
    use futures::FutureExt;
    use serde_json::json;
    use sqlx::Row;
    use temp_dir::TempDir;
    use uuid::Uuid;

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
        assert_eq!(events.len(), 2);

        let run = sqlx::query(
            "SELECT id, name, description, status,
                updated_at, created_at
                FROM chronicle_runs",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            run.get::<Uuid, _>(0),
            Uuid::from_str("00000000-0000-0000-0000-000000000005").unwrap(),
            "run id"
        );
        assert_eq!(run.get::<String, _>(1), "test run", "name");
        assert_eq!(
            run.get::<Option<String>, _>(2),
            Some("test description".to_string()),
            "description"
        );
        assert_eq!(run.get::<String, _>(3), "finished", "status");
        assert_eq!(
            run.get::<DateTime<Utc>, _>(4),
            Utc.timestamp_opt(5, 0).unwrap(),
            "updated_at"
        );
        assert_eq!(
            run.get::<DateTime<Utc>, _>(5),
            Utc.timestamp_opt(1, 0).unwrap(),
            "created_at"
        );

        let steps = sqlx::query(
            "SELECT id, run_id, type, parent_step, name,
                status, start_time, end_time
                FROM chronicle_steps
                ORDER BY start_time ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching steps");
        assert_eq!(steps.len(), 2);

        let step1 = &steps[0];
        assert_eq!(
            step1.get::<Uuid, _>(0),
            Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap(),
            "id"
        );
        assert_eq!(
            step1.get::<Uuid, _>(1),
            Uuid::from_str("00000000-0000-0000-0000-000000000005").unwrap(),
            "run_id"
        );
        assert_eq!(step1.get::<String, _>(2), "step_type", "type");
        assert_eq!(step1.get::<Option<String>, _>(3), None, "parent_step");
        assert_eq!(step1.get::<String, _>(4), "source_node1", "name");
        assert_eq!(step1.get::<String, _>(5), "finished", "status");
        assert_eq!(
            step1.get::<DateTime<Utc>, _>(6),
            Utc.timestamp_opt(2, 0).unwrap(),
            "start_time"
        );
        assert_eq!(
            step1.get::<DateTime<Utc>, _>(7),
            Utc.timestamp_opt(5, 0).unwrap(),
            "end_time"
        );

        let step2 = &steps[1];
        assert_eq!(
            step2.get::<Uuid, _>(0),
            Uuid::from_str("00000000-0000-0000-0000-000000000002").unwrap(),
            "id"
        );
        assert_eq!(
            step2.get::<Uuid, _>(1),
            Uuid::from_str("00000000-0000-0000-0000-000000000005").unwrap(),
            "run_id"
        );
        assert_eq!(step2.get::<String, _>(2), "llm", "type");
        assert_eq!(
            step2.get::<Option<Uuid>, _>(3),
            Some(Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap()),
            "parent_step"
        );
        assert_eq!(step2.get::<String, _>(4), "source_node2", "name");
        assert_eq!(step2.get::<String, _>(5), "error", "status");
        assert_eq!(
            step2.get::<DateTime<Utc>, _>(6),
            Utc.timestamp_opt(3, 0).unwrap(),
            "start_time"
        );
        assert_eq!(
            step2.get::<DateTime<Utc>, _>(7),
            Utc.timestamp_opt(5, 0).unwrap(),
            "end_time"
        );
    }

    async fn verify_sqlite(pool: SqlitePool) {
        let events = sqlx::query("SELECT * FROM chronicle_events")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(events.len(), 2);

        let run = sqlx::query(
            "SELECT id, name, description, status,
                updated_at, created_at
                FROM chronicle_runs",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            run.get::<String, _>(0),
            "00000000-0000-0000-0000-000000000005",
            "run id"
        );
        assert_eq!(run.get::<String, _>(1), "test run", "name");
        assert_eq!(
            run.get::<Option<String>, _>(2),
            Some("test description".to_string()),
            "description"
        );
        assert_eq!(run.get::<String, _>(3), "finished", "status");
        assert_eq!(run.get::<i64, _>(4), 5, "updated_at");
        assert_eq!(run.get::<i64, _>(5), 1, "created_at");

        let steps = sqlx::query(
            "SELECT id, run_id, type, parent_step, name,
                status, start_time, end_time
                FROM chronicle_steps
                ORDER BY start_time ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching steps");
        assert_eq!(steps.len(), 2);

        let step1 = &steps[0];
        assert_eq!(
            step1.get::<String, _>(0),
            "00000000-0000-0000-0000-000000000001",
            "id"
        );
        assert_eq!(
            step1.get::<String, _>(1),
            "00000000-0000-0000-0000-000000000005",
            "run_id"
        );
        assert_eq!(step1.get::<String, _>(2), "step_type", "type");
        assert_eq!(step1.get::<Option<String>, _>(3), None, "parent_step");
        assert_eq!(step1.get::<String, _>(4), "source_node1", "name");
        assert_eq!(step1.get::<String, _>(5), "finished", "status");
        assert_eq!(step1.get::<i64, _>(6), 2, "start_time");
        assert_eq!(step1.get::<i64, _>(7), 5, "end_time");

        let step2 = &steps[1];
        assert_eq!(
            step2.get::<String, _>(0),
            "00000000-0000-0000-0000-000000000002",
            "id"
        );
        assert_eq!(
            step2.get::<String, _>(1),
            "00000000-0000-0000-0000-000000000005",
            "run_id"
        );
        assert_eq!(step2.get::<String, _>(2), "llm", "type");
        assert_eq!(
            step2.get::<Option<String>, _>(3),
            Some("00000000-0000-0000-0000-000000000001".to_string()),
            "parent_step"
        );
        assert_eq!(step2.get::<String, _>(4), "source_node2", "name");
        assert_eq!(step2.get::<String, _>(5), "error", "status");
        assert_eq!(step2.get::<i64, _>(6), 3, "start_time");
        assert_eq!(step2.get::<i64, _>(7), 5, "end_time");
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
        verify_sqlite(pool).await;
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
        verify_sqlite(pool).await;
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
                "run_id": "01234567-89ab-cdef-0123-456789abcdef",
                "step_id": "abcdef01-2345-6789-abcd-ef0123456789",
                "time": "2023-06-28T10:00:00Z"
            }))
            .send()
            .await
            .unwrap();

        let events = json!({
            "events": [
              {
                  "type": "run:start",
                  "id": "00000000-0000-0000-0000-000000000005",
                  "name": "test run",
                  "description": "test description",
                  "application": "test application",
                  "environment": "test environment",
                  "input": {
                    "query": "abc"
                  },
                  "trace_id": "0123456789abcdef",
                  "span_id": "12345678",
                  "tags": ["tag1", "tag2"],
                  "info": {
                    "info1": "value1",
                    "info2": "value2"
                  },
                  "time": "1970-01-01T00:00:01Z"
                  },
              {
                  "type": "step:start",
                  "step_id": "00000000-0000-0000-0000-000000000001",
                  "run_id": "00000000-0000-0000-0000-000000000005",
                  "time": "1970-01-01T00:00:02Z",
                  "data": {
                    "name": "source_node1",
                    "type": "step_type",
                    "span_id": "11111111",
                    "info": {
                      "model": "a_model"
                    },
                    "tags": ["dag", "node"],
                    "input": {
                      "task_param": "value"
                    }
                  }
              },
              {
                  "type": "step:start",
                  "step_id": "00000000-0000-0000-0000-000000000002",
                  "run_id": "00000000-0000-0000-0000-000000000005",
                  "time": "1970-01-01T00:00:03Z",
                  "data": {
                    "name": "source_node2",
                    "type": "llm",
                    "parent_step": "00000000-0000-0000-0000-000000000001",
                    "span_id": "22222222",
                    "info": {
                      "model": "a_model"
                    },
                    "tags": [],
                    "input": {
                      "task_param2": "value"
                    }
                  }
              },
              {
                  "type": "an_event",
                  "data": {
                    "key": "value"
                  },
                  "error": {
                    "message": "something went wrong"
                  },
                  "step_id": "00000000-0000-0000-0000-000000000002",
                  "run_id": "00000000-0000-0000-0000-000000000005",
                  "time": "1970-01-01T00:00:05Z"
              },
              {
                  "type": "step:error",
                  "step_id": "00000000-0000-0000-0000-000000000002",
                  "run_id": "00000000-0000-0000-0000-000000000005",
                  "time": "1970-01-01T00:00:05Z",
                  "data": {
                    "error": {
                      "message": "an error"
                    }
                  }
              },
              {
                  "type": "step:end",
                  "step_id": "00000000-0000-0000-0000-000000000001",
                  "run_id": "00000000-0000-0000-0000-000000000005",
                  "time": "1970-01-01T00:00:05Z",
                  "data": {
                    "output": {
                      "result": "success"
                    },
                    "info": {
                      "info3": "value3"
                    }
                  }
              },
              {
                  "type": "run:update",
                  "id": "00000000-0000-0000-0000-000000000005",
                  "status": "finished",
                  "output": {
                    "result": "success"
                  },
                  "info": {
                    "info2": "new_value",
                    "info3": "value3"
                  },
                  "time": "1970-01-01T00:00:05Z"
                }
            ]
        });

        client
            .post(&format!("{base_url}/events"))
            .json(&events)
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
