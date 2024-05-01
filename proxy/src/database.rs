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

// SQLite's JSON support sucks in 3.44 which sqlx currently includes, so it's not
// possible to just do this via json_group_array(json_object(...)) right now since
// the values in the array are strings of JSON instead of normal JSON..
#[cfg(feature = "sqlite")]
pub async fn load_aliases_from_database(
    pool: &Pool,
    alias_table: &str,
    providers_table: &str,
) -> Result<Vec<AliasConfig>, Report<Error>> {
    use itertools::Itertools;

    #[derive(sqlx::FromRow)]
    struct AliasRow {
        id: i64,
        name: String,
        random_order: bool,
    }

    let aliases: Vec<AliasRow> = sqlx::query_as(&format!(
        "SELECT id, name, random_order FROM {alias_table} ORDER BY id"
    ))
    .fetch_all(pool)
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
    .fetch_all(pool)
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

#[cfg(feature = "postgres")]
pub async fn load_aliases_from_database(
    pool: &Pool,
    alias_table: &str,
    providers_table: &str,
) -> Result<Vec<AliasConfig>, Report<Error>> {
    let json_object = if cfg!(feature = "postgres") {
        "array_agg(jsonb_build_object"
    } else {
        // Once SQLite 3.45 is bundled we can just do this
        "json_group_array(json_object"
    };

    let results = sqlx::query_as::<_, AliasConfig>(&format!(
        "SELECT name, random_order,
                {json_object}(
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
    .attach_printable("Failed to load aliases from database")?;

    #[cfg(feature = "sqlite")]
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

    Ok(results)
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
