//! Application-level error types for the embedding server.
//!
//! This module defines a structured error type hierarchy using `thiserror`
//! for consistent error handling across the server.
//!
//! ## Error Categories
//!
//! - **Model Errors**: Model loading and operation failures
//! - **Authentication Errors**: API key validation and authentication
//! - **Request Errors**: Invalid input or unsupported operations
//! - **Server Errors**: Database, TLS, and startup failures
//! - **Rate Limiting**: Request throttling errors
//!
//! ## Error Types
//!
//! Each error variant maps to an OpenAI-compatible error type (e.g.,
//! "invalid_request_error", "authentication_error") for API consistency.
//!
//! ## Examples
//!
//! ```
//! use static_embedding_server::server::errors::AppError;
//!
//! // Model loading error
//! let err = AppError::ModelLoad("potion-32M".to_string(), "file not found".to_string());
//! assert_eq!(err.error_type(), "model_load_error");
//!
//! // Authentication error
//! let err = AppError::AuthFailed;
//! assert_eq!(err.error_type(), "authentication_error");
//! ```

use thiserror::Error;

/// Application-level errors with structured error types.
#[derive(Error, Debug)]
pub enum AppError {
    /// Model failed to load from disk or remote source.
    #[error("Failed to load model '{0}': {1}")]
    ModelLoad(String, String),

    /// No models were successfully loaded on server startup.
    #[error("No models available")]
    NoModelsAvailable,

    /// API key authentication failed.
    #[error("Authentication failed")]
    AuthFailed,

    /// API key does not match expected format.
    #[error("Invalid API key format")]
    InvalidApiKeyFormat,

    /// API key not found in database.
    #[error("API key not found")]
    ApiKeyNotFound,

    /// Failed to revoke API key.
    #[error("API key revocation failed for key '{0}'")]
    ApiKeyRevocationFailed(String),

    /// Request contains invalid input data.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// API key exceeded rate limit.
    #[error("Rate limit exceeded for API key '{0}'")]
    RateLimitExceeded(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// TLS certificate/key loading or configuration failed.
    #[error("TLS configuration error: {0}")]
    TlsConfigError(String),

    /// Server failed to start due to port conflict or other issue.
    #[error("Server startup error: {0}")]
    StartupError(String),
}

impl AppError {
    /// Get the OpenAI-compatible error type string.
    ///
    /// # Returns
    ///
    /// Static string representing the error category for API responses
    ///
    /// # Examples
    ///
    /// ```
    /// # use static_embedding_server::server::errors::AppError;
    /// let err = AppError::InvalidInput("empty text".to_string());
    /// assert_eq!(err.error_type(), "invalid_request_error");
    /// ```
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