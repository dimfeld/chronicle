use std::sync::Arc;

use chrono::{DateTime, Utc};
use error_stack::{Report, ResultExt};
use itertools::Itertools;
use sqlx::{Row, SqliteExecutor, SqlitePool};
use uuid::Uuid;

use super::{logging::ProxyLogEntry, DbProvider, ProxyDatabase};
use crate::{
    config::{AliasConfig, AliasConfigProvider, ApiKeyConfig},
    workflow_events::{RunEndEvent, RunStartEvent, StepEvent, StepEventData, StepStartData},
    Error,
};

const SQLITE_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_sqlite.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_sqlite.sql"),
    include_str!("../../migrations/20240625_chronicle_proxy_steps_sqlite.sql"),
];

#[derive(Debug)]
pub struct SqliteDatabase {
    pub pool: SqlitePool,
}

impl SqliteDatabase {
    pub fn new(pool: SqlitePool) -> Arc<dyn ProxyDatabase> {
        Arc::new(Self { pool })
    }

    async fn write_step_start(
        &self,
        tx: impl SqliteExecutor<'_>,
        step_id: Uuid,
        run_id: Uuid,
        data: StepStartData,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r##"
            INSERT INTO chronicle_steps (
                id, run_id, type, parent_step, name, input, status, tags, info, span_id, start_time
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, 'started', $7, $8, $9, $10
            )
            ON CONFLICT DO NOTHING;
            "##,
        )
        .bind(step_id.to_string())
        .bind(run_id.to_string())
        .bind(data.typ)
        .bind(data.parent_step.map(|s| s.to_string()))
        .bind(data.name)
        .bind(data.input)
        .bind(data.tags.join("|"))
        .bind(data.info)
        .bind(data.span_id)
        .bind(timestamp.unwrap_or_else(|| Utc::now()).timestamp())
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_step_end(
        &self,
        tx: impl SqliteExecutor<'_>,
        step_id: Uuid,
        run_id: Uuid,
        status: &str,
        output: serde_json::Value,
        info: Option<serde_json::Value>,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        // TODO this needs a different method of merging JSON info since the current
        // query uses Postgres syntax.
        sqlx::query(
            r##"
            UPDATE chronicle_steps
            SET status = $1,
                output = $2,
                info = CASE
                    WHEN NULLIF(info, 'null') IS NULL THEN $3
                    WHEN NULLIF($3, 'null') IS NULL THEN info
                    ELSE json_patch(info, $3)
                    END,
                end_time = $4
            WHERE run_id = $5 AND id = $6
        "##,
        )
        .bind(status)
        .bind(output)
        .bind(info)
        .bind(timestamp.unwrap_or_else(|| chrono::Utc::now()).timestamp())
        .bind(run_id.to_string())
        .bind(step_id.to_string())
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_step_event(
        &self,
        tx: impl SqliteExecutor<'_>,
        entry: StepEvent,
    ) -> Result<(), sqlx::Error> {
        match entry.data {
            StepEventData::Start(data) => {
                self.write_step_start(tx, entry.step_id, entry.run_id, data, entry.time)
                    .await?;
            }
            StepEventData::End(data) => {
                self.write_step_end(
                    tx,
                    entry.step_id,
                    entry.run_id,
                    "finished",
                    data.output,
                    data.info,
                    entry.time,
                )
                .await?;
            }
            StepEventData::Error(data) => {
                self.write_step_end(
                    tx,
                    entry.step_id,
                    entry.run_id,
                    "error",
                    data.error,
                    None,
                    entry.time,
                )
                .await?;
            }
            StepEventData::State(data) => {
                sqlx::query(
                    "UPDATE chronicle_steps
                    SET status=$1
                    WHERE run_id=$2 AND id=$3",
                )
                .bind(data.state)
                .bind(entry.run_id.to_string())
                .bind(entry.step_id.to_string())
                .execute(tx)
                .await?;
            }
        }

        Ok(())
    }

    async fn write_run_start(
        &self,
        tx: impl SqliteExecutor<'_>,
        event: RunStartEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r##"
            INSERT INTO chronicle_runs (
                id, name, description, application, environment, input, status,
                    trace_id, span_id, tags, info, updated_at, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, 'started', $7, $8, $9, $10, $11, $11
            );
            "##,
        )
        .bind(event.id.to_string())
        .bind(event.name)
        .bind(event.description)
        .bind(event.application)
        .bind(event.environment)
        .bind(event.input)
        .bind(event.trace_id)
        .bind(event.span_id)
        .bind(event.tags.join("|"))
        .bind(event.info)
        .bind(event.time.unwrap_or_else(|| Utc::now()).timestamp())
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_run_end(
        &self,
        tx: impl SqliteExecutor<'_>,
        event: RunEndEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chronicle_runs
            SET status = $1,
                output = $2,
                info = CASE
                    WHEN NULLIF(info, 'null') IS NULL THEN $3
                    WHEN NULLIF($3, 'null') IS NULL THEN info
                    ELSE json_patch(info, $3)
                    END,
                updated_at = $4
            WHERE id = $5",
        )
        .bind(event.status.as_deref().unwrap_or("finished"))
        .bind(event.output)
        .bind(event.info)
        .bind(event.time.unwrap_or_else(|| Utc::now()).timestamp())
        .bind(event.id.to_string())
        .execute(tx)
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProxyDatabase for SqliteDatabase {
    async fn load_providers_from_database(
        &self,
        providers_table: &str,
    ) -> Result<Vec<DbProvider>, Report<crate::Error>> {
        let rows: Vec<DbProvider> = sqlx::query_as(&format!(
            "SELECT name, label, url, api_key, format, headers, prefix, api_key_source
        FROM {providers_table}"
        ))
        .fetch_all(&self.pool)
        .await
        .change_context(Error::LoadingDatabase)
        .attach_printable("Failed to load providers from database")?;

        Ok(rows)
    }

    // SQLite's JSON support sucks in 3.44 which sqlx currently includes, so it's not
    // possible to just do this via json_group_array(json_object(...)) right now since
    // the values in the array are strings of JSON instead of normal JSON. Next version
    // of sqlx will include 3.45 which works better and we can make this look more like
    // the Postgres version.
    async fn load_aliases_from_database(
        &self,
        alias_table: &str,
        providers_table: &str,
    ) -> Result<Vec<AliasConfig>, Report<Error>> {
        #[derive(sqlx::FromRow)]
        struct AliasRow {
            id: i64,
            name: String,
            random_order: bool,
        }

        let aliases: Vec<AliasRow> = sqlx::query_as(&format!(
            "SELECT id, name, random_order FROM {alias_table} ORDER BY id"
        ))
        .fetch_all(&self.pool)
        .await
        .change_context(Error::LoadingDatabase)?;

        #[derive(sqlx::FromRow, Debug)]
        struct DbAliasConfigProvider {
            alias_id: i64,
            provider: String,
            model: String,
            api_key_name: Option<String>,
        }
        let models: Vec<DbAliasConfigProvider> = sqlx::query_as(&format!(
            "SELECT alias_id, provider, model, api_key_name
        FROM {providers_table}
        JOIN {alias_table} ON {alias_table}.id = {providers_table}.alias_id
        ORDER BY alias_id, sort"
        ))
        .fetch_all(&self.pool)
        .await
        .change_context(Error::LoadingDatabase)?;

        let mut output = Vec::with_capacity(aliases.len());
        let mut aliases = aliases.into_iter();
        let mut models = models.into_iter().peekable();

        while let Some(alias) = aliases.next() {
            let models = models
                .by_ref()
                .peeking_take_while(|model| model.alias_id == alias.id)
                .map(|model| AliasConfigProvider {
                    provider: model.provider,
                    model: model.model,
                    api_key_name: model.api_key_name,
                })
                .collect();
            output.push(AliasConfig {
                name: alias.name,
                random_order: alias.random_order,
                models,
            });
        }

        Ok(output)
    }

    async fn load_api_key_configs_from_database(
        &self,
        table: &str,
    ) -> Result<Vec<ApiKeyConfig>, Report<Error>> {
        let rows: Vec<ApiKeyConfig> =
            sqlx::query_as(&format!("SELECT name, source, value FROM {table}"))
                .fetch_all(&self.pool)
                .await
                .change_context(Error::LoadingDatabase)
                .attach_printable("Failed to load API keys from database")?;

        Ok(rows)
    }

    async fn write_log_batch(&self, entries: Vec<ProxyLogEntry>) -> Result<(), sqlx::Error> {
        let mut event_builder = sqlx::QueryBuilder::new(super::logging::EVENT_INSERT_PREFIX);
        let mut tx = self.pool.begin().await?;
        let mut first_event = true;

        for entry in entries.into_iter() {
            match entry {
                ProxyLogEntry::Event(item) => {
                    let (rmodel, rprovider, rbody, rmeta) = match item
                        .response
                        .map(|r| (r.body.model.clone(), r.provider, r.body, r.info.meta))
                    {
                        Some((rmodel, rprovider, rbody, rmeta)) => {
                            (rmodel, Some(rprovider), Some(rbody), rmeta)
                        }
                        None => (None, None, None, None),
                    };

                    let model = rmodel
                        .or_else(|| item.request.as_ref().and_then(|r| r.model.clone()))
                        .unwrap_or_default();

                    let extra = item.options.metadata.extra.filter(|m| !m.is_empty());

                    if first_event {
                        first_event = false;
                    } else {
                        event_builder.push(",");
                    }

                    let mut tuple = event_builder.separated(",");
                    tuple
                        .push_unseparated("(")
                        // sqlx encodes UUIDs as binary blobs by default with Sqlite, which is often nice
                        // but not what we want here.
                        .push_bind(item.id.to_string())
                        .push_bind(item.event_type)
                        .push_bind(item.options.internal_metadata.organization_id)
                        .push_bind(item.options.internal_metadata.project_id)
                        .push_bind(item.options.internal_metadata.user_id)
                        .push_bind(sqlx::types::Json(item.request))
                        .push_bind(sqlx::types::Json(rbody))
                        .push_bind(sqlx::types::Json(item.error))
                        .push_bind(rprovider)
                        .push_bind(model)
                        .push_bind(item.options.metadata.application)
                        .push_bind(item.options.metadata.environment)
                        .push_bind(item.options.metadata.organization_id)
                        .push_bind(item.options.metadata.project_id)
                        .push_bind(item.options.metadata.user_id)
                        .push_bind(item.options.metadata.workflow_id)
                        .push_bind(item.options.metadata.workflow_name)
                        .push_bind(item.options.metadata.run_id)
                        .push_bind(item.options.metadata.step)
                        .push_bind(item.options.metadata.step_index.map(|i| i as i32))
                        .push_bind(item.options.metadata.prompt_id)
                        .push_bind(item.options.metadata.prompt_version.map(|i| i as i32))
                        .push_bind(sqlx::types::Json(extra))
                        .push_bind(rmeta)
                        .push_bind(item.num_retries.map(|n| n as i32))
                        .push_bind(item.was_rate_limited)
                        .push_bind(item.latency.map(|d| d.as_millis() as i64))
                        .push_bind(item.total_latency.map(|d| d.as_millis() as i64))
                        .push_bind(item.timestamp.timestamp())
                        .push_unseparated(")");
                }
                ProxyLogEntry::StepEvent(event) => {
                    self.write_step_event(&mut *tx, event).await?;
                }
                ProxyLogEntry::RunStart(event) => {
                    self.write_run_start(&mut *tx, event).await?;
                }
                ProxyLogEntry::RunEnd(event) => {
                    self.write_run_end(&mut *tx, event).await?;
                }
            }
        }

        if !first_event {
            let query = event_builder.build();
            query.execute(&mut *tx).await?;
        }

        tx.commit().await?;

        Ok(())
    }
}

/// Run database migrations specific to the proxy. These migrations are designed for a simple setup with
/// single-tenant use. You may want to add multi-tenant features or partitioning, and can integrate
/// the files from the `migrations` directory into your project to accomplish that.
pub async fn run_default_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS chronicle_meta (
          key text PRIMARY KEY,
          value text
        );",
    )
    .execute(&mut *tx)
    .await?;

    let migration_version = sqlx::query_scalar::<_, i32>(
        "SELECT cast(value as int) FROM chronicle_meta WHERE key='migration_version'",
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(0) as usize;

    tracing::info!("Migration version is {}", migration_version);

    let start_migration = migration_version.min(SQLITE_MIGRATIONS.len());
    for (i, migration) in SQLITE_MIGRATIONS[start_migration..].iter().enumerate() {
        tracing::info!("Running migration {}", start_migration + i);
        sqlx::raw_sql(migration).execute(&mut *tx).await?;
    }

    let new_version = SQLITE_MIGRATIONS.len();

    sqlx::query("UPDATE chronicle_meta SET value=$1 WHERE key='migration_version'")
        .bind(new_version.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use serde_json::json;
    use sqlx::Row;

    use crate::database::{
        sqlite::run_default_migrations,
        testing::{test_events, TEST_RUN_ID, TEST_STEP1_ID, TEST_STEP2_ID},
    };

    #[sqlx::test(migrations = false)]
    async fn test_database_writes(pool: sqlx::SqlitePool) {
        filigree::tracing_config::test::init();
        run_default_migrations(&pool).await.unwrap();

        let db = super::SqliteDatabase::new(pool.clone());

        db.write_log_batch(test_events())
            .await
            .expect("Writing events");

        let runs = sqlx::query(
            "SELECT id, name, description, application, environment,
                input, output, status, trace_id, span_id,
                tags, info, updated_at, created_at
                FROM chronicle_runs",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching runs");
        assert_eq!(runs.len(), 1);
        let run = &runs[0];

        assert_eq!(run.get::<String, _>(0), TEST_RUN_ID.to_string(), "run id");
        assert_eq!(run.get::<String, _>(1), "test run", "name");
        assert_eq!(
            run.get::<Option<String>, _>(2),
            Some("test description".to_string()),
            "description"
        );
        assert_eq!(
            run.get::<Option<String>, _>(3),
            Some("test application".to_string()),
            "application"
        );
        assert_eq!(
            run.get::<Option<String>, _>(4),
            Some("test environment".to_string()),
            "environment"
        );
        assert_eq!(
            run.get::<Option<serde_json::Value>, _>(5),
            Some(json!({"query":"abc"})),
            "input"
        );
        assert_eq!(
            run.get::<Option<serde_json::Value>, _>(6),
            Some(json!({"result":"success"})),
            "output"
        );
        assert_eq!(run.get::<String, _>(7), "finished", "status");
        assert_eq!(
            run.get::<Option<String>, _>(8),
            Some("0123456789abcdef".to_string()),
            "trace_id"
        );
        assert_eq!(
            run.get::<Option<String>, _>(9),
            Some("12345678".to_string()),
            "span_id"
        );
        assert_eq!(
            run.get::<Option<String>, _>(10),
            Some("tag1|tag2".to_string()),
            "tags"
        );
        assert_eq!(
            run.get::<Option<serde_json::Value>, _>(11),
            Some(json!({"info1":"value1","info2":"new_value", "info3":"value3"})),
            "info"
        );
        assert_eq!(run.get::<i64, _>(12), 5, "updated_at");
        assert_eq!(run.get::<i64, _>(13), 1, "created_at");

        let steps = sqlx::query(
            "SELECT id, run_id, type, parent_step, name,
                input, output, status, span_id, tags, info, start_time, end_time
                FROM chronicle_steps",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching steps");
        assert_eq!(steps.len(), 2);

        let step1 = &steps[0];
        assert_eq!(step1.get::<String, _>(0), TEST_STEP1_ID.to_string(), "id");
        assert_eq!(step1.get::<String, _>(1), TEST_RUN_ID.to_string(), "run_id");
        assert_eq!(step1.get::<String, _>(2), "step_type", "type");
        assert_eq!(step1.get::<Option<String>, _>(3), None, "parent_step");
        assert_eq!(step1.get::<String, _>(4), "source_node1", "name");
        assert_eq!(
            step1.get::<Option<serde_json::Value>, _>(5),
            Some(json!({ "task_param": "value"})),
            "input"
        );
        assert_eq!(
            step1.get::<Option<serde_json::Value>, _>(6),
            Some(json!({ "result": "success" })),
            "output"
        );
        assert_eq!(step1.get::<String, _>(7), "finished", "status");
        assert_eq!(
            step1.get::<Option<String>, _>(8),
            Some("11111111".to_string()),
            "span_id"
        );
        assert_eq!(
            step1.get::<Option<String>, _>(9),
            Some("dag|node".to_string()),
            "tags"
        );
        assert_eq!(
            step1.get::<Option<serde_json::Value>, _>(10),
            Some(json!({"model": "a_model", "info3": "value3"})),
            "info"
        );
        assert_eq!(step1.get::<i64, _>(11), 2, "start_time");
        assert_eq!(step1.get::<i64, _>(12), 5, "end_time");

        let step2 = &steps[1];
        assert_eq!(step2.get::<String, _>(0), TEST_STEP2_ID.to_string(), "id");
        assert_eq!(step2.get::<String, _>(1), TEST_RUN_ID.to_string(), "run_id");
        assert_eq!(step2.get::<String, _>(2), "llm", "type");
        assert_eq!(
            step2.get::<Option<String>, _>(3),
            Some(TEST_STEP1_ID.to_string()),
            "parent_step"
        );
        assert_eq!(step2.get::<String, _>(4), "source_node2", "name");
        assert_eq!(
            step2.get::<Option<serde_json::Value>, _>(5),
            Some(json!({ "task_param2": "value"})),
            "input"
        );
        assert_eq!(
            step2.get::<Option<serde_json::Value>, _>(6),
            Some(json!({ "message": "an error" })),
            "output"
        );
        assert_eq!(step2.get::<String, _>(7), "error", "status");
        assert_eq!(
            step2.get::<Option<String>, _>(8),
            Some("22222222".to_string()),
            "span_id"
        );
        assert_eq!(
            step2.get::<Option<String>, _>(9),
            Some("".to_string()),
            "tags"
        );
        assert_eq!(
            step2.get::<Option<serde_json::Value>, _>(10),
            Some(json!({"model": "a_model"})),
            "info"
        );
        assert_eq!(step2.get::<i64, _>(11), 3, "start_time");
        assert_eq!(step2.get::<i64, _>(12), 4, "end_time");
    }
}
