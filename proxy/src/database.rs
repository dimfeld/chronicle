use std::{collections::BTreeMap, sync::Arc};

use error_stack::Report;
use logging::ProxyLogEntry;

use crate::{
    config::{AliasConfig, ApiKeyConfig, CustomProviderConfig},
    providers::custom::ProviderRequestFormat,
    Error,
};

pub mod logging;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(test)]
mod testing;

/// A DBMS-agnostic interface to a database
#[async_trait::async_trait]
pub trait ProxyDatabase: std::fmt::Debug + Send + Sync {
    /// Load provider configuration from the database
    async fn load_providers_from_database(
        &self,
        providers_table: &str,
    ) -> Result<Vec<DbProvider>, Report<Error>>;

    /// Load alias configuration from the database
    async fn load_aliases_from_database(
        &self,
        alias_table: &str,
        providers_table: &str,
    ) -> Result<Vec<AliasConfig>, Report<Error>>;

    /// Load API key configuration from the database
    async fn load_api_key_configs_from_database(
        &self,
        table: &str,
    ) -> Result<Vec<ApiKeyConfig>, Report<Error>>;

    /// Write a batch of log entries to the database
    async fn write_log_batch(&self, items: Vec<ProxyLogEntry>) -> Result<(), sqlx::Error>;
}

/// A [ProxyDatabase] wrapped in an [Arc]
pub type Database = Arc<dyn ProxyDatabase>;

/// A provider configuration loaded from the database
#[derive(sqlx::FromRow)]
pub struct DbProvider {
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
    db: &dyn ProxyDatabase,
    providers_table: &str,
) -> Result<Vec<CustomProviderConfig>, Report<Error>> {
    let rows = db.load_providers_from_database(providers_table).await?;
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
