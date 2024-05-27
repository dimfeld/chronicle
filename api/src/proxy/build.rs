//! Build a proxy from the database

use std::sync::Arc;

use chronicle_proxy::{
    database::{load_providers_from_database, ProxyDatabase},
    Proxy,
};
use error_stack::{Report, ResultExt};
use sqlx::PgPool;

use crate::Error;

pub async fn build_proxy(pool: PgPool) -> Result<Proxy, Report<Error>> {
    let db = Arc::new(chronicle_proxy::database::postgres::PostgresDatabase { pool });
    let aliases = db
        .load_aliases_from_database("aliases", "alias_models")
        .await
        .change_context(Error::BuildingProxy)?;
    let providers = load_providers_from_database(db.as_ref(), "custom_providers")
        .await
        .change_context(Error::BuildingProxy)?;
    let api_keys = db
        .load_api_key_configs_from_database("provider_api_keys")
        .await
        .change_context(Error::BuildingProxy)?;

    Proxy::builder()
        .with_database(db)
        // We use our own tables here
        .load_config_from_database(false)
        .with_aliases(aliases)
        .with_api_keys(api_keys)
        .with_custom_providers(providers)
        .log_to_database(true)
        .build()
        .await
        .change_context(Error::BuildingProxy)
}
