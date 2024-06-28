/// Proxy errors
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A specified model provider was not found
    #[error("Unknown model provider {0}")]
    UnknownProvider(String),

    /// No provider was specified, and no default could be inferred from the model name
    #[error("No default provider for model {0}")]
    NoDefault(String),

    /// An alias references a provider that doesn't exist
    #[error("Alias {0} references nonexistent provider {1}")]
    NoAliasProvider(String, String),

    /// An alias does not reference any models
    #[error("Alias {0} has no associated models")]
    AliasEmpty(String),

    /// An alias references an API key that doesn't exist
    #[error("Alias {0} references nonexistent API key {1}")]
    NoAliasApiKey(String, String),

    /// The requested API key does not exist
    #[error("Unknown API key name {0}")]
    NoApiKey(String),

    /// The request is missing the model name
    #[error("Request is missing model name")]
    ModelNotSpecified,

    /// The model provider returned an error
    #[error("Model provider returned an error")]
    ModelError,

    /// The API key was not provided
    #[error("API key not provided")]
    MissingApiKey,

    /// The environment variable for the API key was not found
    #[error("Did not find environment variable {1} for API key {0}")]
    MissingApiKeyEnv(String, String),

    /// Failed to parse the model provider's output
    #[error("Failed to parse model provider's output")]
    ResultParseError,

    /// Failed to read the configuration file
    #[error("Failed to read configuration file")]
    ReadingConfig,

    /// Failed to load data from the database
    #[error("Failed to load from the database")]
    LoadingDatabase,

    /// Failed to parse a header value
    #[error("Failed to parse header value {0}: Expected a {1}")]
    ReadingHeader(String, &'static str),

    /// A required piece of information was missing from the response stream
    #[error("Did not see {0} in response stream")]
    MissingStreamInformation(&'static str),
}
