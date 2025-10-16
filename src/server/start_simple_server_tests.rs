// Tests for start_http_server in start_simple.rs
// These tests use minimal AppState and mock model loading to avoid real dependencies.

#[cfg(test)]
mod start_simple_server_tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use crate::server::state::AppState;
    use anyhow::Result;
    use tokio::runtime::Runtime;

    // Helper to run async test in sync context
    fn run_async<F: std::future::Future<Output = T>, T>(f: F) -> T {
        let rt = Runtime::new().unwrap();
        rt.block_on(f)
    }

    #[test]
    fn test_start_http_server_invalid_address() {
        let result = run_async(start_http_server("bad_addr", 10, 10));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Failed to parse bind address"));
    }

    #[test]
    fn test_start_http_server_no_models() {
        // Patch AppState::new to return error
        // This is a placeholder: in real code, use a mocking framework
        // For now, just check that the error propagates if AppState::new fails
        // (since real model loading will fail in test env)
        let result = run_async(start_http_server("127.0.0.1:9999", 10, 10));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Failed to initialize app state"));
    }

    #[test]
    fn test_start_http_server_bind_error() {
        // Try to bind to a privileged port (likely to fail)
        let result = run_async(start_http_server("127.0.0.1:1", 10, 10));
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Failed to bind to address"));
    }
}
