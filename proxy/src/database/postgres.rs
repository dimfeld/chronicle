use std::sync::Arc;

use error_stack::{Report, ResultExt};
use sqlx::PgPool;

use super::{logging::ProxyLogEntry, DbProvider, ProxyDatabase};
use crate::{
    config::{AliasConfig, ApiKeyConfig},
    Error,
};

const POSTGRESQL_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_postgresql.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_postgresql.sql"),
];

#[derive(Debug)]
pub struct PostgresDatabase {
    pub pool: PgPool,
}

impl PostgresDatabase {
    pub fn new(pool: PgPool) -> Arc<dyn ProxyDatabase> {
        Arc::new(Self { pool })
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

    async fn write_log_batch(
        &self,
        query: String,
        items: Vec<ProxyLogEntry>,
    ) -> Result<(), sqlx::Error> {
        let mut query = sqlx::query(&query);

        for item in items.into_iter() {
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

            query = query
                .bind(item.id)
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
                .bind(item.latency.map(|d| d.as_millis() as i64))
                .bind(item.total_latency.map(|d| d.as_millis() as i64))
                .bind(item.timestamp);
        }

        query.execute(&self.pool).await?;
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
