use error_stack::{Report, ResultExt};
use itertools::Itertools;
use sqlx::SqlitePool;

use super::{logging::ProxyLogEntry, DbProvider, ProxyDatabase};
use crate::{
    config::{AliasConfig, AliasConfigProvider, ApiKeyConfig},
    Error,
};

const SQLITE_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_sqlite.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_sqlite.sql"),
];

#[derive(Debug)]
pub struct SqliteDatabase {
    pub pool: SqlitePool,
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

    async fn write_log_batch(
        &self,
        query: String,
        items: Vec<ProxyLogEntry>,
    ) -> Result<(), sqlx::Error> {
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

            query = query
                // sqlx encodes UUIDs as binary blobs by default with Sqlite, which is often nice
                // but not what we want here.
                .bind(item.id.to_string())
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

        query.execute(&self.pool).await?;
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
