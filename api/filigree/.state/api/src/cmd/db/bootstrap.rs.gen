use clap::Args;
use error_stack::Report;
use sqlx::PgPool;

use crate::Error;

#[derive(Args, Debug)]
pub struct BootstrapCommand {
    /// The email for the admin user
    #[clap(long = "email", env = "BOOTSTRAP_ADMIN_EMAIL")]
    admin_email: String,

    /// The name for the admin user
    /// Defaults to "Admin"
    #[clap(long = "name", env = "BOOTSTRAP_ADMIN_NAME")]
    admin_name: Option<String>,

    /// A password for the admin user, prehashed with Argon2. The `util hash-password` subcommand can be used
    /// to generate a password hash if you don't have one already. If omitted, login through OAuth2 and passwordless methods
    /// will still work.
    #[clap(
        long = "password-hash",
        env = "BOOTSTRAP_ADMIN_PASSWORD_HASH",
        conflicts_with = "admin_password"
    )]
    admin_password_hash: Option<String>,

    /// A plain-text password for the admin user. If omitted, login through OAuth2 and passwordless methods
    /// will still work.
    #[clap(
        long = "password",
        env = "BOOTSTRAP_ADMIN_PASSWORD",
        conflicts_with = "admin_password_hash"
    )]
    admin_password: Option<String>,

    /// The name for the admin user's organization.
    /// Defaults to "Administration"
    #[clap(long = "org-name", env = "BOOTSTRAP_ORG_NAME")]
    organization_name: Option<String>,

    /// Force adding the admin user even if the database already contains at least one
    /// organization.
    #[clap(long, env = "BOOTSTRAP_FORCE")]
    force: bool,
}

impl BootstrapCommand {
    pub async fn handle(self, pg_pool: PgPool) -> Result<(), Report<Error>> {
        let password = match (self.admin_password_hash, self.admin_password) {
            (Some(hash), _) => Some(filigree::auth::password::HashedPassword(hash)),
            (None, Some(pass)) => {
                let hash = filigree::auth::password::new_hash(pass)
                    .await
                    .map_err(Error::from)?;
                Some(hash)
            }
            (None, None) => None,
        };

        let data = crate::db::BootstrapData {
            force: self.force,
            admin_email: self.admin_email,
            admin_name: self.admin_name,
            admin_password: password,
            organization_name: self.organization_name,
        };

        let bootstrapped = crate::db::bootstrap(pg_pool, data).await?;
        if bootstrapped {
            println!("Bootstrapped database");
        } else {
            println!("Database already bootstrapped");
        }

        Ok(())
    }
}
