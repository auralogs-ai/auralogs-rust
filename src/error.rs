use thiserror::Error;

pub type Result<T> = std::result::Result<T, AuralogError>;

#[derive(Debug, Error)]
pub enum AuralogError {
    #[error("api_key is required")]
    MissingApiKey,
    #[error("environment is required")]
    MissingEnvironment,
    #[error("endpoint is required")]
    MissingEndpoint,
    #[error("auralog global client is already initialized")]
    AlreadyInitialized,
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("transport error: {0}")]
    Transport(#[from] std::io::Error),
}
