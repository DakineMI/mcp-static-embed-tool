//! HTTP utility handlers for health checks and monitoring.
//!
//! This module provides lightweight endpoints for infrastructure monitoring:
//! - **GET /health**: Simple health check endpoint
//! - Returns 200 OK if server is running
//!
//! ## Use Cases
//!
//! - Load balancer health checks
//! - Container orchestration (Kubernetes liveness/readiness probes)
//! - Monitoring and alerting systems
//!
//! ## Examples
//!
//! ```bash
//! # Check server health
//! curl http://localhost:8080/health
//! # Returns: 200 OK (no body)
//! ```

use axum::http::StatusCode;

/// Health check endpoint for load balancer health status checking.
///
/// Returns 200 OK if the server process is running. Does not check:
/// - Model availability (use `/v1/models` instead)
/// - Database connectivity
/// - External service dependencies
///
/// # Returns
///
/// HTTP 200 OK status code with no body
///
/// # Examples
///
/// ```
/// # use static_embedding_server::server::http::health;
/// # use axum::http::StatusCode;
/// # #[tokio::main]
/// # async fn main() {
/// let status = health().await;
/// assert_eq!(status, StatusCode::OK);
/// # }
/// ```
pub async fn health() -> StatusCode {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let status = health().await;
        assert_eq!(status, StatusCode::OK);
    }
}
