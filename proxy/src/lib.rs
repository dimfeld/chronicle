use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};

pub mod anthropic;
pub mod error;
pub mod ollama;
pub mod openai;

pub mod database;

use database::Pool;
pub use error::Error;

#[async_trait::async_trait]
pub trait ChatModelProvider: Debug + Send + Sync {
    fn name(&self) -> &str;

    async fn send_request(
        &self,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error>;

    fn default_url(&self) -> &'static str;

    fn is_default_for_model(&self, model: &str) -> bool;
}

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
