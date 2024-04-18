use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};

pub mod database;
pub mod error;
pub mod providers;

use database::Pool;
pub use error::Error;
use providers::ChatModelProvider;

#[derive(Debug)]
pub struct Proxy {
    pool: Option<database::Pool>,
    config_path: Option<PathBuf>,
    providers: Vec<Arc<dyn ChatModelProvider>>,
}

impl Proxy {
    pub async fn new(
        database_pool: Option<Pool>,
        config_path: Option<PathBuf>,
    ) -> Result<Self, Error> {
        // todo load the providers from the database and from the config file if present

        Ok(Self {
            pool: database_pool,
            config_path,
            providers: default_providers(),
        })
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
    }

    pub fn default_for_model(&self, model: &str) -> Option<Arc<dyn ChatModelProvider>> {
        self.providers
            .iter()
            .find(|p| p.is_default_for_model(model))
            .map(Arc::clone)
    }
}

impl Proxy {}

pub fn default_providers() -> Vec<Arc<dyn ChatModelProvider>> {
    vec![]
}
