use std::sync::Arc;

use chrono::{DateTime, Utc};
use error_stack::{Report, ResultExt};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{logging::ProxyLogEntry, DbProvider, ProxyDatabase};
use crate::{
    config::{AliasConfig, ApiKeyConfig},
    workflow_events::{RunEndEvent, RunStartEvent, StepEvent, StepEventData, StepStartData},
    Error,
};

const POSTGRESQL_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_postgresql.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_postgresql.sql"),
    include_str!("../../migrations/20240625_chronicle_proxy_steps_postgresql.sql"),
];

#[derive(Debug)]
pub struct PostgresDatabase {
    pub pool: PgPool,
}

impl PostgresDatabase {
    pub fn new(pool: PgPool) -> Arc<dyn ProxyDatabase> {
        Arc::new(Self { pool })
    }

    async fn write_step_start(
        &self,
        tx: impl PgExecutor<'_>,
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
        .bind(step_id)
        .bind(run_id)
        .bind(data.typ)
        .bind(data.parent_step)
        .bind(data.name)
        .bind(data.input)
        .bind(data.tags)
        .bind(data.info)
        .bind(data.span_id)
        .bind(timestamp.unwrap_or_else(|| Utc::now()))
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
                updated_at = $4
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

    async fn write_step_event(
        &self,
        tx: impl PgExecutor<'_>,
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
                .bind(entry.run_id)
                .bind(entry.step_id)
                .execute(tx)
                .await?;
            }
        }

        Ok(())
    }

    async fn write_run_start(
        &self,
        tx: impl PgExecutor<'_>,
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
        .bind(event.tags.join("|"))
        .bind(event.info)
        .bind(event.time.unwrap_or_else(|| Utc::now()))
        .execute(tx)
        .await?;
        Ok(())
    }

    async fn write_run_end(
        &self,
        tx: impl PgExecutor<'_>,
        event: RunEndEvent,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE chronicle_runs
            SET status = $1,
                output = $2,
                updated_at = $3
            WHERE id = $4",
        )
        .bind(event.status.as_deref().unwrap_or("finished"))
        .bind(event.output)
        .bind(event.time.unwrap_or_else(|| Utc::now()))
        .bind(event.id)
        .execute(tx)
        .await?;
        Ok(())
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

        /*
        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        let results = results
            .into_iter()
            .map(|row| {
                let models = serde_json::from_str(&row.models)
                    .change_context(Error::LoadingDatabase)
                    .attach_printable_lazy(|| {
                        format!("Invalid model definition linked to alias {}", row.name)
                    })?;

                let alias = AliasConfig {
                    name: row.name,
                    random_order: row.random_order,
                    models,
                };
                Ok::<AliasConfig, Report<Error>>(alias)
            })
            .collect::<Result<Vec<_>, _>>()?;
        */

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
                        .push_bind(item.timestamp)
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
