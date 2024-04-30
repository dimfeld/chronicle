use std::collections::BTreeMap;

use error_stack::{Report, ResultExt};

use crate::{
    config::{AliasConfig, AliasConfigProvider, ApiKeyConfig, CustomProviderConfig},
    providers::custom::ProviderRequestFormat,
    Error,
};

pub mod logging;
#[cfg(feature = "migrations")]
pub mod migrations;

#[cfg(feature = "postgres")]
pub type Database = sqlx::Postgres;

#[cfg(feature = "sqlite")]
pub type Database = sqlx::Sqlite;

pub type Pool = sqlx::Pool<Database>;

#[derive(sqlx::FromRow)]
struct DbProvider {
    name: String,
    label: Option<String>,
    url: String,
    api_key: Option<String>,
    format: sqlx::types::Json<ProviderRequestFormat>,
    headers: Option<sqlx::types::Json<BTreeMap<String, String>>>,
    prefix: Option<String>,
    api_key_source: Option<String>,
}

/// Load provider configuration from the database
pub async fn load_providers_from_database(
    pool: &Pool,
    providers_table: &str,
) -> Result<Vec<CustomProviderConfig>, Report<Error>> {
    let rows: Vec<DbProvider> = sqlx::query_as(&format!(
        "SELECT name, label, url, api_key, format, headers, prefix, api_key_source
        FROM {providers_table}"
    ))
    .fetch_all(pool)
    .await
    .change_context(Error::LoadingDatabase)
    .attach_printable("Failed to load providers from database")?;

    let providers = rows
        .into_iter()
        .map(|row| CustomProviderConfig {
            name: row.name,
            label: row.label,
            url: row.url,
            api_key: row.api_key,
            format: row.format.0,
            headers: row.headers.unwrap_or_default().0,
            prefix: row.prefix,
            api_key_source: row.api_key_source,
        })
        .collect();
    Ok(providers)
}

pub async fn load_aliases_from_database(
    pool: &Pool,
    alias_table: &str,
    providers_table: &str,
) -> Result<Vec<AliasConfig>, Report<Error>> {
    sqlx::query_as(&format!(
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
    .fetch_all(pool)
    .await
    .change_context(Error::LoadingDatabase)
    .attach_printable("Failed to load aliases from database")
}

pub async fn load_api_key_configs_from_database(
    pool: &Pool,
    table: &str,
) -> Result<Vec<ApiKeyConfig>, Report<Error>> {
    let rows: Vec<ApiKeyConfig> =
        sqlx::query_as(&format!("SELECT name, source, value FROM {table}"))
            .fetch_all(pool)
            .await
            .change_context(Error::LoadingDatabase)
            .attach_printable("Failed to load API keys from database")?;

    Ok(rows)
}
