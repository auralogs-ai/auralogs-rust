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
    #[error("invalid config: {0}")]
    InvalidConfig(String),
}
