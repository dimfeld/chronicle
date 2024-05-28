use std::str::FromStr;

use chronicle_proxy::database::Database;
use error_stack::Report;
use sqlx::sqlite::SqliteConnectOptions;

pub async fn init_database(db: Option<String>) -> Result<Option<Database>, Report<sqlx::Error>> {
    let Some(db) = db else {
        tracing::info!("No database configured");
        return Ok(None);
    };

    let pg = db.starts_with("postgresql://") || db.starts_with("postgres://");

    if pg {
        Ok(Some(init_pg(db).await?))
    } else {
        Ok(Some(init_sqlite(db).await?))
    }
}

async fn init_pg(db: String) -> Result<Database, Report<sqlx::Error>> {
    tracing::info!("Connecting to PostgreSQL database at {db}");
    let pool = sqlx::postgres::PgPoolOptions::new().connect(&db).await?;
    chronicle_proxy::database::postgres::run_default_migrations(&pool).await?;

    Ok(chronicle_proxy::database::postgres::PostgresDatabase::new(
        pool,
    ))
}

async fn init_sqlite(db: String) -> Result<Database, Report<sqlx::Error>> {
    tracing::info!("Opening SQLite database at {db}");

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::from_str(&db)?
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .create_if_missing(true),
        )
        .await?;

    chronicle_proxy::database::sqlite::run_default_migrations(&pool).await?;

    Ok(chronicle_proxy::database::sqlite::SqliteDatabase::new(pool))
}
