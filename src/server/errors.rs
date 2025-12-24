//! Application-level error types for the embedding server.
//!
//! This module defines a structured error type hierarchy using `thiserror`
//! for consistent error handling across the server.
//!
//! ## Error Categories
//! 
//! - **Model Errors**: Model loading and operation failures
//! - **Request Errors**: Invalid input or unsupported operations
//! - **Server Errors**: Database and startup failures
//! 
//! ## Error Types
//! 
//! Each error variant maps to an OpenAI-compatible error type (e.g.,
//! "invalid_request_error", "server_error") for API consistency.
//! 
//! ## Examples
//! 
//! ```
//! use static_embedding_tool::server::errors::AppError;
//! 
//! // Model loading error
//! let err = AppError::ModelLoad("potion-32M".to_string(), "file not found".to_string());
//! assert_eq!(err.error_type(), "model_load_error");
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

    /// Request contains invalid input data.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Database operation failed.
    #[error("Database error: {0}")]
    DatabaseError(String),

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
    /// # use static_embedding_tool::server::errors::AppError;
    /// let err = AppError::InvalidInput("empty text".to_string());
    /// assert_eq!(err.error_type(), "invalid_request_error");
    /// ```
    pub fn error_type(&self) -> &'static str {
        match self {
            AppError::ModelLoad(_, _) => "model_load_error",
            AppError::NoModelsAvailable => "server_error",
            AppError::InvalidInput(_) => "invalid_request_error",
            AppError::DatabaseError(_) => "server_error",
            AppError::StartupError(_) => "server_error",
        }
    }

    pub fn code(&self) -> Option<&'static str> {
        match self {
            AppError::InvalidInput(_) => Some("invalid_input"),            _ => None,
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
    }

    #[test]
    fn test_app_error_error_type() {
        assert_eq!(AppError::ModelLoad("test".to_string(), "error".to_string()).error_type(), "model_load_error");
        assert_eq!(AppError::NoModelsAvailable.error_type(), "server_error");
        assert_eq!(AppError::InvalidInput("bad input".to_string()).error_type(), "invalid_request_error");
        assert_eq!(AppError::DatabaseError("db error".to_string()).error_type(), "server_error");
    }

    #[test]
    fn test_app_error_code() {
        assert_eq!(AppError::InvalidInput("test".to_string()).code(), Some("invalid_input"));
         
        // Test errors that return None
        assert_eq!(AppError::ModelLoad("test".to_string(), "error".to_string()).code(), None);
        assert_eq!(AppError::NoModelsAvailable.code(), None);
    }
}