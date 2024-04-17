use clap::{Args, Subcommand};
use error_stack::{Report, ResultExt};

use crate::Error;

mod bootstrap;

#[derive(Args, Debug)]
pub struct DbCommand {
    /// The PostgreSQL database to connect to
    #[clap(long = "db", env = "DATABASE_URL")]
    database_url: String,

    #[clap(subcommand)]
    pub command: DbSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DbSubcommand {
    /// Create the initial set of data in the database.
    Bootstrap(bootstrap::BootstrapCommand),
    /// Update the database with the latest migrations
    Migrate,
}

impl DbCommand {
    pub async fn handle(self) -> Result<(), Report<Error>> {
        let pg_pool = sqlx::PgPool::connect(&self.database_url)
            .await
            .change_context(Error::Db)?;

        match self.command {
            DbSubcommand::Bootstrap(cmd) => cmd.handle(pg_pool).await,
            DbSubcommand::Migrate => crate::db::run_migrations(&pg_pool).await,
        }
    }
}
