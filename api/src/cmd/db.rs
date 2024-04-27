use clap::{Args, Subcommand};
use error_stack::{Report, ResultExt};
use filigree::{
    auth::{OrganizationId, RoleId},
    users::{
        roles::{add_roles_to_user, remove_roles_from_user},
        users::CreateUserDetails,
    },
};
use sqlx::PgPool;

use crate::{auth::ANON_USER_ID, users::users::UserCreator, Error};

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
    /// Configure access for anonymous users
    Anon(AnonCommand),
}

impl DbCommand {
    pub async fn handle(self) -> Result<(), Report<Error>> {
        let pg_pool = sqlx::PgPool::connect(&self.database_url)
            .await
            .change_context(Error::Db)?;

        match self.command {
            DbSubcommand::Bootstrap(cmd) => cmd.handle(pg_pool).await,
            DbSubcommand::Anon(cmd) => cmd.handle(pg_pool).await,
            DbSubcommand::Migrate => crate::db::run_migrations(&pg_pool).await,
        }
    }
}

#[derive(Args, Debug)]
pub struct AnonCommand {
    #[clap(subcommand)]
    pub command: AnonSubCommand,
}

#[derive(Debug, Subcommand)]
pub enum AnonSubCommand {
    /// Disable the anonymous user
    Disable,
    /// Allow anonymous users to read the configuration and access analytics.
    Read,
    /// Allow anonymous users to update the configuration.
    Write,
}

impl AnonCommand {
    async fn handle(&self, pool: PgPool) -> Result<(), Report<Error>> {
        match self.command {
            AnonSubCommand::Disable => {
                sqlx::query!(
                    "UPDATE organization_members
                    SET active=false
                    WHERE user_id = $1",
                    ANON_USER_ID.as_uuid()
                )
                .execute(&pool)
                .await
                .change_context(Error::Db)?;
            }
            AnonSubCommand::Read => {
                create_anon_user(&pool, false).await?;
            }
            AnonSubCommand::Write => {
                create_anon_user(&pool, true).await?;
            }
        }

        Ok(())
    }
}

async fn create_anon_user(pool: &PgPool, can_write: bool) -> Result<(), Report<Error>> {
    let mut tx = pool.begin().await.change_context(Error::Db)?;
    // First see if the anon user exists already.
    let existing = sqlx::query!(
        "SELECT organization_id, active
        FROM organization_members
        WHERE user_id=$1",
        ANON_USER_ID.as_uuid()
    )
    .fetch_optional(&mut *tx)
    .await
    .change_context(Error::Db)?;

    let (org, active) = if let Some(existing) = existing {
        let org = OrganizationId::from_uuid(existing.organization_id);
        (org, existing.active)
    } else {
        // The user doesn't exist, so create it.
        // Right now we assume there's just a single organization so it's safe to add it
        // to that one.
        let org = sqlx::query_scalar!("SELECT id FROM organizations LIMIT 1")
            .fetch_one(&mut *tx)
            .await
            .change_context(Error::Db)?;
        let org = OrganizationId::from_uuid(org);

        UserCreator::create_user(
            &mut *tx,
            Some(org),
            CreateUserDetails {
                name: Some("Anonymous User".to_string()),
                ..Default::default()
            },
        )
        .await
        .change_context(Error::Db)?;

        (org, true)
    };

    if !active {
        sqlx::query!(
            "UPDATE organization_members
            SET active=true
            WHERE user_id=$1",
            ANON_USER_ID.as_uuid()
        )
        .execute(&mut *tx)
        .await
        .change_context(Error::Db)?;
    }

    let write_role = sqlx::query_scalar!(
        "SELECT id FROM roles WHERE organization_id=$1 AND name='Writer'",
        org.as_uuid()
    )
    .fetch_one(&mut *tx)
    .await
    .change_context(Error::Db)?;
    let write_role = RoleId::from_uuid(write_role);
    if can_write {
        add_roles_to_user(&mut *tx, org, ANON_USER_ID, &[write_role])
            .await
            .change_context(Error::Db)?;
    } else {
        remove_roles_from_user(&mut *tx, org, ANON_USER_ID, &[write_role])
            .await
            .change_context(Error::Db)?;
    }

    tx.commit().await.change_context(Error::Db)?;
    Ok(())
}
