//! PostgreSQL database logging
use std::sync::Arc;

use chrono::{DateTime, Utc};
use error_stack::{Report, ResultExt};
use sqlx::{PgExecutor, PgPool, QueryBuilder};
use uuid::Uuid;

use super::{
    logging::{ProxyLogEntry, ProxyLogEvent},
    DbProvider, ProxyDatabase,
};
use crate::{
    config::{AliasConfig, ApiKeyConfig},
    workflow_events::{
        RunStartEvent, RunUpdateEvent, StepEventData, StepStartData, StepStateData, WorkflowEvent,
    },
    Error,
};

const POSTGRESQL_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_postgresql.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_postgresql.sql"),
    include_str!("../../migrations/20240625_chronicle_proxy_steps_postgresql.sql"),
];

/// PostgreSQL database support for logging
#[derive(Debug)]
pub struct PostgresDatabase {
    pool: PgPool,
}

impl PostgresDatabase {
    /// Create a new [PostgresDatabase]
    pub fn new(pool: PgPool) -> Arc<dyn ProxyDatabase> {
        Arc::new(Self { pool })
    }

    async fn write_step_start(
        &self,
        tx: impl PgExecutor<'_>,
        event: StepEventData<StepStartData>,
    ) -> Result<(), sqlx::Error> {
        let tags = if event.data.tags.is_empty() {
            None
        } else {
            Some(event.data.tags)
        };

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
        .bind(event.step_id)
        .bind(event.run_id)
        .bind(event.data.typ)
        .bind(event.data.parent_step)
        .bind(event.data.name)
        .bind(event.data.input)
        .bind(tags)
        .bind(event.data.info)
        .bind(event.data.span_id)
        .bind(event.time.unwrap_or_else(|| Utc::now()))
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_step_end(
        &self,
        tx: impl PgExecutor<'_>,
        step_id: Uuid,
        run_id: Uuid,
        status: &str,
        output: serde_json::Value,
        info: Option<serde_json::Value>,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r##"
            UPDATE chronicle_steps
            SET status = $1,
                output = $2,
                info = CASE
                    WHEN NULLIF(info, 'null'::jsonb) IS NULL THEN $3
                    WHEN NULLIF($3, 'null'::jsonb) IS NULL THEN info
                    ELSE info || $3
                    END,
                end_time = $4
            WHERE run_id = $5 AND id = $6
        "##,
        )
        .bind(status)
        .bind(output)
        .bind(info)
        .bind(timestamp.unwrap_or_else(|| chrono::Utc::now()))
        .bind(run_id)
        .bind(step_id)
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_step_status(
        &self,
        tx: impl PgExecutor<'_>,
        event: StepEventData<StepStateData>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chronicle_steps
                    SET status=$1
                    WHERE run_id=$2 AND id=$3",
        )
        .bind(event.data.state)
        .bind(event.run_id)
        .bind(event.step_id)
        .execute(tx)
        .await?;

        Ok(())
    }

    async fn write_run_start(
        &self,
        tx: impl PgExecutor<'_>,
        event: RunStartEvent,
    ) -> Result<(), sqlx::Error> {
        let tags = if event.tags.is_empty() {
            None
        } else {
            Some(event.tags)
        };

        sqlx::query(
            r##"
            INSERT INTO chronicle_runs (
                id, name, description, application, environment, input, status,
                    trace_id, span_id, tags, info, updated_at, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, 'started', $7, $8, $9, $10, $11, $11
            )
            ON CONFLICT DO NOTHING;
            "##,
        )
        .bind(event.id)
        .bind(event.name)
        .bind(event.description)
        .bind(event.application)
        .bind(event.environment)
        .bind(event.input)
        .bind(event.trace_id)
        .bind(event.span_id)
        .bind(tags)
        .bind(event.info)
        .bind(event.time.unwrap_or_else(|| Utc::now()))
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_run_update(
        &self,
        tx: impl PgExecutor<'_>,
        event: RunUpdateEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chronicle_runs
            SET status = $1,
                output = $2,
                info = CASE
                    WHEN NULLIF(info, 'null'::jsonb) IS NULL THEN $3
                    WHEN NULLIF($3, 'null'::jsonb) IS NULL THEN info
                    ELSE info || $3
                    END,
                updated_at = $4
            WHERE id = $5",
        )
        .bind(event.status.as_deref().unwrap_or("finished"))
        .bind(event.output)
        .bind(event.info)
        .bind(event.time.unwrap_or_else(|| Utc::now()))
        .bind(event.id)
        .execute(tx)
        .await?;
        Ok(())
    }

    fn add_event_values(builder: &mut QueryBuilder<'_, sqlx::Postgres>, item: ProxyLogEvent) {
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

        let mut tuple = builder.separated(",");
        tuple
            .push_unseparated("(")
            .push_bind(item.id)
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
            .push_bind(item.options.metadata.step_id)
            .push_bind(item.options.metadata.step_index.map(|i| i as i32))
            .push_bind(item.options.metadata.prompt_id)
            .push_bind(item.options.metadata.prompt_version.map(|i| i as i32))
            .push_bind(sqlx::types::Json(extra))
            .push_bind(rmeta)
            .push_bind(item.num_retries.map(|n| n as i32))
            .push_bind(item.was_rate_limited)
            .push_bind(item.latency.map(|d| d.as_millis() as i64))
            .push_bind(item.total_latency.map(|d| d.as_millis() as i64))
            .push_bind(item.timestamp)
            .push_unseparated(")");
    }
}

#[async_trait::async_trait]
impl ProxyDatabase for PostgresDatabase {
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

    async fn load_aliases_from_database(
        &self,
        alias_table: &str,
        providers_table: &str,
    ) -> Result<Vec<AliasConfig>, Report<Error>> {
        let results = sqlx::query_as::<_, AliasConfig>(&format!(
            "SELECT name, random_order,
                array_agg(jsonb_build_object(
                'provider', ap.provider,
                'model', ap.model,
                'api_key_name', ap.api_key_name
                ) order by ap.sort) as models
            FROM {alias_table} al
            JOIN {providers_table} ap ON ap.alias_id = al.id
            GROUP BY al.id",
        ))
        .fetch_all(&self.pool)
        .await
        .change_context(Error::LoadingDatabase)
        .attach_printable("Failed to load aliases from database")?;

        Ok(results)
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
                ProxyLogEntry::Proxied(item) => {
                    if first_event {
                        first_event = false;
                    } else {
                        event_builder.push(",");
                    }

                    Self::add_event_values(&mut event_builder, *item);
                }
                ProxyLogEntry::Workflow(WorkflowEvent::Event(event)) => {
                    if first_event {
                        first_event = false;
                    } else {
                        event_builder.push(",");
                    }

                    let item = ProxyLogEvent::from_payload(Uuid::now_v7(), event);
                    Self::add_event_values(&mut event_builder, item);
                }
                ProxyLogEntry::Workflow(WorkflowEvent::StepStart(event)) => {
                    self.write_step_start(&mut *tx, event).await?;
                }

                ProxyLogEntry::Workflow(WorkflowEvent::StepEnd(event)) => {
                    self.write_step_end(
                        &mut *tx,
                        event.step_id,
                        event.run_id,
                        "finished",
                        event.data.output,
                        event.data.info,
                        event.time,
                    )
                    .await?;
                }
                ProxyLogEntry::Workflow(WorkflowEvent::StepState(event)) => {
                    self.write_step_status(&mut *tx, event).await?;
                }
                ProxyLogEntry::Workflow(WorkflowEvent::StepError(event)) => {
                    self.write_step_end(
                        &mut *tx,
                        event.step_id,
                        event.run_id,
                        "error",
                        event.data.error,
                        None,
                        event.time,
                    )
                    .await?;
                }
                ProxyLogEntry::Workflow(WorkflowEvent::RunStart(event)) => {
                    self.write_run_start(&mut *tx, event).await?;
                }
                ProxyLogEntry::Workflow(WorkflowEvent::RunUpdate(event)) => {
                    self.write_run_update(&mut *tx, event).await?;
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
pub async fn run_default_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS chronicle_meta (
          key text PRIMARY KEY,
          value jsonb
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

    let start_migration = migration_version.min(POSTGRESQL_MIGRATIONS.len());
    for (i, migration) in POSTGRESQL_MIGRATIONS[start_migration..].iter().enumerate() {
        tracing::info!("Running migration {}", start_migration + i);
        sqlx::raw_sql(migration).execute(&mut *tx).await?;
    }

    let new_version = POSTGRESQL_MIGRATIONS.len();

    sqlx::query("UPDATE chronicle_meta SET value=$1::jsonb WHERE key='migration_version'")
        .bind(new_version.to_string())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::json;
    use sqlx::{PgPool, Row};
    use uuid::Uuid;

    use crate::database::{
        postgres::run_default_migrations,
        testing::{test_events, TEST_EVENT1_ID, TEST_RUN_ID, TEST_STEP1_ID, TEST_STEP2_ID},
    };

    fn dt(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    #[sqlx::test(migrations = false)]
    async fn test_database_writes(pool: PgPool) {
        filigree::tracing_config::test::init();
        run_default_migrations(&pool).await.unwrap();

        let db = super::PostgresDatabase::new(pool.clone());

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

        assert_eq!(run.get::<Uuid, _>(0), TEST_RUN_ID, "run id");
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
            run.get::<Vec<String>, _>(10),
            vec!["tag1".to_string(), "tag2".to_string()],
            "tags"
        );
        assert_eq!(
            run.get::<Option<serde_json::Value>, _>(11),
            Some(json!({"info1":"value1","info2":"new_value", "info3":"value3"})),
            "info"
        );
        assert_eq!(run.get::<DateTime<Utc>, _>(12), dt(5), "updated_at");
        assert_eq!(run.get::<DateTime<Utc>, _>(13), dt(1), "created_at");

        let steps = sqlx::query(
            "SELECT id, run_id, type, parent_step, name,
                input, output, status, span_id, tags, info, start_time, end_time
                FROM chronicle_steps
                ORDER BY start_time ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching steps");
        assert_eq!(steps.len(), 2);

        let step1 = &steps[0];
        assert_eq!(step1.get::<Uuid, _>(0), TEST_STEP1_ID, "id");
        assert_eq!(step1.get::<Uuid, _>(1), TEST_RUN_ID, "run_id");
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
            step1.get::<Option<Vec<String>>, _>(9),
            Some(vec!["dag".to_string(), "node".to_string()]),
            "tags"
        );
        assert_eq!(
            step1.get::<Option<serde_json::Value>, _>(10),
            Some(json!({"model": "a_model", "info3": "value3"})),
            "info"
        );
        assert_eq!(
            step1.get::<DateTime<Utc>, _>(11),
            Utc.timestamp_opt(2, 0).unwrap(),
            "start_time"
        );
        assert_eq!(
            step1.get::<DateTime<Utc>, _>(12),
            Utc.timestamp_opt(5, 0).unwrap(),
            "end_time"
        );

        let step2 = &steps[1];
        assert_eq!(step2.get::<Uuid, _>(0), TEST_STEP2_ID, "id");
        assert_eq!(step2.get::<Uuid, _>(1), TEST_RUN_ID, "run_id");
        assert_eq!(step2.get::<String, _>(2), "llm", "type");
        assert_eq!(
            step2.get::<Option<Uuid>, _>(3),
            Some(TEST_STEP1_ID),
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
        assert_eq!(step2.get::<Option<Vec<String>>, _>(9), None, "tags");
        assert_eq!(
            step2.get::<Option<serde_json::Value>, _>(10),
            Some(json!({"model": "a_model"})),
            "info"
        );
        assert_eq!(
            step2.get::<DateTime<Utc>, _>(11),
            Utc.timestamp_opt(3, 0).unwrap(),
            "start_time"
        );
        assert_eq!(
            step2.get::<DateTime<Utc>, _>(12),
            Utc.timestamp_opt(5, 0).unwrap(),
            "end_time"
        );

        let events = sqlx::query(
            "SELECT id, event_type, step_id, run_id, meta, error, created_at
                FROM chronicle_events
                ORDER BY created_at ASC",
        )
        .fetch_all(&pool)
        .await
        .expect("Fetching steps");
        assert_eq!(events.len(), 2);

        let event = &events[0];
        assert_eq!(event.get::<Uuid, _>(0), TEST_EVENT1_ID, "id");
        assert_eq!(event.get::<String, _>(1), "query", "event_type");
        assert_eq!(event.get::<Uuid, _>(2), TEST_STEP2_ID, "step_id");
        assert_eq!(event.get::<Uuid, _>(3), TEST_RUN_ID, "run_id");
        assert_eq!(
            event.get::<Option<serde_json::Value>, _>(4),
            Some(json!({"some_key": "some_value"})),
            "meta"
        );
        assert_eq!(
            event.get::<Option<serde_json::Value>, _>(5),
            Some(json!(null)),
            "error"
        );
        assert_eq!(event.get::<DateTime<Utc>, _>(6), dt(4), "created_at");

        let event2 = &events[1];
        assert_eq!(event2.get::<String, _>(1), "an_event", "event_type");
        assert_eq!(event2.get::<Uuid, _>(2), TEST_STEP2_ID, "step_id");
        assert_eq!(event2.get::<Uuid, _>(3), TEST_RUN_ID, "run_id");
        assert_eq!(
            event2.get::<Option<serde_json::Value>, _>(4),
            Some(json!({"key": "value"})),
            "meta"
        );
        assert_eq!(
            event2.get::<Option<serde_json::Value>, _>(5),
            Some(json!({ "message": "something went wrong"})),
            "error"
        );
        assert_eq!(event2.get::<DateTime<Utc>, _>(6), dt(5), "created_at");
    }
}
