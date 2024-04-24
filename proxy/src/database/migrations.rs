//! Database migrations

use super::Pool;

#[cfg(feature = "sqlite")]
const SQLITE_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_sqlite.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_sqlite.sql"),
];

#[cfg(feature = "postgres")]
const POSTGRESQL_MIGRATIONS: &[&'static str] = &[
    include_str!("../../migrations/20240419_chronicle_proxy_init_postgresql.sql"),
    include_str!("../../migrations/20240424_chronicle_proxy_data_tables_postgresql.sql"),
];

/// Run database migrations specific to the proxy. These migrations are designed for a simple setup with
/// single-tenant use. You may want to add multi-tenant features or partitioning, and can integrate
/// the files from the `migrations` directory into your project to accomplish that.
pub async fn run_default_migrations(pool: &Pool) -> Result<(), sqlx::Error> {
    #[cfg(feature = "sqlite")]
    run_migrations(pool, SQLITE_MIGRATIONS).await?;

    #[cfg(feature = "postgres")]
    run_migrations(pool, POSTGRESQL_MIGRATIONS).await?;

    Ok(())
}

async fn run_migrations(pool: &Pool, migrations: &[&str]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let migration_version = sqlx::query_scalar::<_, i32>(
        "SELECT value::int FROM chronicle_meta WHERE key='migration_version'",
    )
    .fetch_optional(&mut *tx)
    .await
    .ok()
    .flatten()
    .unwrap_or(0) as usize;

    let start_migration = migration_version.min(migrations.len());
    for migration in &migrations[start_migration..] {
        sqlx::query(migration).execute(&mut *tx).await?;
    }

    let new_version = migrations.len();

    sqlx::query(
        "UPDATE chronicle_meta
            SET value=$1::jsonb WHERE key='migration_version",
    )
    .bind(new_version as i32)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}
