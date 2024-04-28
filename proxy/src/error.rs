#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unknown model provider {0}")]
    UnknownProvider(String),
    #[error("No default provider for model {0}")]
    NoDefault(String),
    #[error("Alias {0} references nonexistent provider {1}")]
    NoAliasProvider(String, String),
    #[error("Alias {0} has no associated models")]
    AliasEmpty(String),
    #[error("Alias {0} references nonexistent API key {1}")]
    NoAliasApiKey(String, String),
    #[error("Unknown API key name {0}")]
    NoApiKey(String),
    #[error("Request is missing model name")]
    ModelNotSpecified,
    #[error("Model provider returned an error")]
    ModelError,
    #[error("API key not provided")]
    MissingApiKey,
    #[error("Did not find environment variable {1} for API key {0}")]
    MissingApiKeyEnv(String, String),
    #[error("Error transforming a model request")]
    TransformingRequest,
    #[error("Error transforming a model response")]
    TransformingResponse,
    #[error("Failed to parse model provider's output")]
    ResultParseError,
    #[error("Failed to read configuration file")]
    ReadingConfig,
    #[error("Failed to load from the database")]
    LoadingDatabase,
}
