use clap::{Args, Subcommand};
use error_stack::{Report, ResultExt};
use schemars::schema_for;

use crate::Error;

#[derive(Args, Debug)]
pub struct UtilCommand {
    #[clap(subcommand)]
    pub command: UtilSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum UtilSubcommand {
    HashPassword(HashPasswordCommand),
}

#[derive(Args, Debug)]
pub struct HashPasswordCommand {
    password: String,
}

impl UtilCommand {
    pub async fn handle(self) -> Result<(), Report<Error>> {
        match self.command {
            UtilSubcommand::HashPassword(password) => {
                let hash = filigree::auth::password::new_hash(password.password)
                    .await
                    .change_context(Error::AuthSubsystem)?
                    .0;
                println!("{hash}");
            }
        }

        Ok(())
    }
}
