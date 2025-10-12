use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Failed to load model '{0}': {1}")]
    ModelLoad(String, String),

    #[error("No models available")]
    NoModelsAvailable,

    #[error("Authentication failed")]
    AuthFailed,

    #[error("Invalid API key format")]
    InvalidApiKeyFormat,

    #[error("API key not found")]
    ApiKeyNotFound,

    #[error("API key revocation failed for key '{0}'")]
    ApiKeyRevocationFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Rate limit exceeded for API key '{0}'")]
    RateLimitExceeded(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("TLS configuration error: {0}")]
    TlsConfigError(String),

    #[error("Server startup error: {0}")]
    StartupError(String),
}

impl AppError {
    pub fn error_type(&self) -> &'static str {
        match self {
            AppError::ModelLoad(_, _) => "model_load_error",
            AppError::NoModelsAvailable => "server_error",
            AppError::AuthFailed => "authentication_error",
            AppError::InvalidApiKeyFormat => "invalid_request_error",
            AppError::ApiKeyNotFound => "authentication_error",
            AppError::ApiKeyRevocationFailed(_) => "server_error",
            AppError::InvalidInput(_) => "invalid_request_error",
            AppError::RateLimitExceeded(_) => "rate_limit_error",
            AppError::DatabaseError(_) => "server_error",
            AppError::TlsConfigError(_) => "server_error",
            AppError::StartupError(_) => "server_error",
        }
    }

    pub fn code(&self) -> Option<&'static str> {
        match self {
            AppError::InvalidInput(_) => Some("invalid_input"),
            AppError::RateLimitExceeded(_) => Some("rate_limit_exceeded"),
            AppError::AuthFailed => Some("auth_failed"),
            AppError::InvalidApiKeyFormat => Some("invalid_api_key"),
            _ => None,
        }
    }
}