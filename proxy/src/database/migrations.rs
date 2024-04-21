//! Database migrations

use super::Pool;

#[cfg(feature = "sqlite")]
const SQLITE_MIGRATIONS: &[&'static str] = &[include_str!(
    "../../migrations/20140419_chronicle_proxy_init_sqlite.sql"
)];

#[cfg(feature = "sqlite")]
pub async fn init_sqlite_db(pool: &Pool) -> Result<(), sqlx::Error> {
    run_migrations(pool, SQLITE_MIGRATIONS).await?;
}

#[cfg(feature = "postgres")]
const POSTGRESQL_MIGRATIONS: &[&'static str] = &[include_str!(
    "../../migrations/20240419_chronicle_proxy_init_postgresql.sql"
)];

#[cfg(feature = "postgres")]
pub async fn init_postgresql_db(pool: &Pool) -> Result<(), sqlx::Error> {
    run_migrations(pool, POSTGRESQL_MIGRATIONS).await
}

#[cfg(not(all(feature = "sqlite", feature = "postgres")))]
pub async fn init_db(pool: &Pool) -> Result<(), sqlx::Error> {
    #[cfg(feature = "sqlite")]
    init_sqlite_db(pool).await?;

    #[cfg(feature = "postgres")]
    init_postgresql_db(pool).await?;

    Ok(())
}

pub async fn run_migrations(pool: &Pool, migrations: &[&str]) -> Result<(), sqlx::Error> {
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
