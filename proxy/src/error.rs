use crate::providers::ProviderError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unknown model provider {0}")]
    UnknownProvider(String),
    #[error("No default provider for model {0}")]
    NoDefault(String),
    #[error("Request is missing model name")]
    ModelNotSpecified,
    #[error(transparent)]
    ModelError(ProviderError),
    #[error("Error transforming a model request")]
    TransformingRequest,
    #[error("Error transforming a model response")]
    TransformingResponse,
    #[error("Failed to parse model provider's output")]
    ResultParseError,
}
