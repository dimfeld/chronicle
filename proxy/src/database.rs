use std::collections::BTreeMap;

use error_stack::{Report, ResultExt};

use crate::{
    config::{AliasConfig, AliasConfigProvider, ApiKeyConfig, CustomProviderConfig},
    providers::custom::ProviderRequestFormat,
    Error,
};

mod any_layer;
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
    token: Option<String>,
    format: sqlx::types::Json<ProviderRequestFormat>,
    headers: Option<sqlx::types::Json<BTreeMap<String, String>>>,
    prefix: Option<String>,
    default_for: Option<sqlx::types::Json<Vec<String>>>,
    token_env: Option<String>,
}

/// Load provider configuration from the database
pub async fn load_providers_from_database(
    pool: &Pool,
    providers_table: &str,
) -> Result<Vec<CustomProviderConfig>, Report<Error>> {
    let rows: Vec<DbProvider> = sqlx::query_as(&format!(
        "SELECT name, label, url, token, format, headers, prefix, default_for, token_env
        FROM {providers_table}"
    ))
    .fetch_all(pool)
    .await
    .change_context(Error::LoadingDatabase)?;

    let providers = rows
        .into_iter()
        .map(|row| CustomProviderConfig {
            name: row.name,
            label: row.label,
            url: row.url,
            api_key: row.token,
            format: row.format.0,
            headers: row.headers.unwrap_or_default().0,
            prefix: row.prefix,
            default_for: row.default_for.unwrap_or_default().0,
            api_key_source: row.token_env,
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
}

pub async fn load_api_key_configs_from_database(
    pool: &Pool,
    table: &str,
) -> Result<Vec<ApiKeyConfig>, Report<Error>> {
    let rows: Vec<ApiKeyConfig> =
        sqlx::query_as(&format!("SELECT name, source, value FROM {table}"))
            .fetch_all(pool)
            .await
            .change_context(Error::LoadingDatabase)?;

    Ok(rows)
}
