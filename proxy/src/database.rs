#[cfg(feature = "any-db")]
pub type Database = sqlx::Any;

#[cfg(all(not(feature = "any-db"), feature = "postgres"))]
pub type Database = sqlx::Postgres;

#[cfg(all(not(feature = "any-db"), feature = "sqlite"))]
pub type Database = sqlx::Sqlite;

pub type Pool = sqlx::Pool<Database>;
