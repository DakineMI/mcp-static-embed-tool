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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_display() {
        let error = AppError::ModelLoad("test-model".to_string(), "load failed".to_string());
        assert_eq!(error.to_string(), "Failed to load model 'test-model': load failed");

        let error = AppError::NoModelsAvailable;
        assert_eq!(error.to_string(), "No models available");

        let error = AppError::AuthFailed;
        assert_eq!(error.to_string(), "Authentication failed");
    }

    #[test]
    fn test_app_error_error_type() {
        assert_eq!(AppError::ModelLoad("test".to_string(), "error".to_string()).error_type(), "model_load_error");
        assert_eq!(AppError::NoModelsAvailable.error_type(), "server_error");
        assert_eq!(AppError::AuthFailed.error_type(), "authentication_error");
        assert_eq!(AppError::InvalidApiKeyFormat.error_type(), "invalid_request_error");
        assert_eq!(AppError::ApiKeyNotFound.error_type(), "authentication_error");
        assert_eq!(AppError::ApiKeyRevocationFailed("key".to_string()).error_type(), "server_error");
        assert_eq!(AppError::InvalidInput("bad input".to_string()).error_type(), "invalid_request_error");
        assert_eq!(AppError::RateLimitExceeded("key".to_string()).error_type(), "rate_limit_error");
        assert_eq!(AppError::DatabaseError("db error".to_string()).error_type(), "server_error");
        assert_eq!(AppError::TlsConfigError("tls error".to_string()).error_type(), "server_error");
        assert_eq!(AppError::StartupError("startup error".to_string()).error_type(), "server_error");
    }

    #[test]
    fn test_app_error_code() {
        assert_eq!(AppError::InvalidInput("test".to_string()).code(), Some("invalid_input"));
        assert_eq!(AppError::RateLimitExceeded("key".to_string()).code(), Some("rate_limit_exceeded"));
        assert_eq!(AppError::AuthFailed.code(), Some("auth_failed"));
        assert_eq!(AppError::InvalidApiKeyFormat.code(), Some("invalid_api_key"));
        
        // Test errors that return None
        assert_eq!(AppError::ModelLoad("test".to_string(), "error".to_string()).code(), None);
        assert_eq!(AppError::NoModelsAvailable.code(), None);
        assert_eq!(AppError::ApiKeyNotFound.code(), None);
    }
}