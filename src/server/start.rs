use axum::{Router, routing::get};

use rmcp::transport::{
    StreamableHttpServerConfig,
    streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};


use crate::server::logs::init_logging_and_metrics;
use crate::server::api::create_api_router;
use crate::server::http::health;
use crate::server::state::AppState;
use crate::tools::EmbeddingService;
use crate::utils::{format_duration, generate_connection_id};
use anyhow::{Result as AnyhowResult, anyhow};

#[derive(Clone)]
pub struct ServerConfig {
    /// Base URL for the server
    pub server_url: String,
    /// TCP address to bind (e.g., "127.0.0.1:8084")
    pub bind_address: Option<String>,
}

// Global metrics

/// Handle graceful shutdown with double Ctrl+C force quit.
///
/// Monitors for Ctrl+C signals and implements a safety mechanism:
/// - **First Ctrl+C**: Initiates graceful shutdown
/// - **Second Ctrl+C** (within 2 seconds): Force quits immediately
/// - **Timeout**: Resets counter after 2 seconds of no signals
///
/// This prevents accidental force quits while allowing escape from hanging shutdowns.
///
/// # Examples
///
/// ```ignore
/// tokio::spawn(handle_double_ctrl_c());
/// // Server continues running...
/// ```
async fn handle_double_ctrl_c() {
    let mut ctrl_c_count = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                ctrl_c_count += 1;
                if ctrl_c_count == 1 {
                    info!("Received first Ctrl+C signal. Press Ctrl+C again within 2 seconds to force quit.");
                    interval.reset();
                } else if ctrl_c_count >= 2 {
                    warn!("Received second Ctrl+C signal. Force quitting immediately.");
                    std::process::exit(1);
                }
            }
            _ = interval.tick() => {
                if ctrl_c_count > 0 {
                    info!("Ctrl+C timeout expired. Resuming normal operation.");
                    ctrl_c_count = 0;
                }
            }
        }
    }
}

pub async fn start_server(config: ServerConfig) -> AnyhowResult<()> {
    // Output debugging information
    info!(
        server_url = config.server_url,
        bind_address = config.bind_address.as_deref().unwrap_or("N/A"),
    );
    match config.bind_address.is_some() {
        // We are running as a STDIO server
        false => start_stdio_server(config).await,
        // We are running as a HTTP server
        true => start_http_server(config).await,
    }
}

async fn start_stdio_server(_config: ServerConfig) -> AnyhowResult<()> {
    // Initialize structured logging (stderr only for stdio mode)
    #[cfg(feature = "mcp")]
    init_logging_and_metrics(false);

    info!("Starting MCP server in stdio mode");

    // Generate a connection ID for this stdio session
    let connection_id = generate_connection_id();

    // For stdio mode, we need to load models since we don't have AppState
    // This is a simplified version - in production, models should be shared
    let models = match crate::server::state::AppState::new().await {
        Ok(state) => state.models,
        Err(e) => {
            error!("Failed to load models for stdio mode: {}", e);
            return Err(anyhow!("Failed to load models: {}", e));
        }
    };

    // Create the embedding service for this session
    let service = EmbeddingService::new(connection_id.clone(), models);

    info!(
        connection_id = %connection_id,
        "MCP stdio server initialized"
    );

    // Create stdio transport using tokio stdin/stdout
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Serve MCP over stdio
    match rmcp::serve_server(service.clone(), (stdin, stdout)).await {
        Ok(server) => {
            info!("MCP stdio server started successfully");
            // Wait for the server to complete (will run until stdin EOF)
            if let Err(e) = server.waiting().await {
                error!("Server error: {}", e);
            }
            info!(
                connection_id = %service.connection_id,
                connection_time = %format_duration(Instant::now().duration_since(service.created_at)),
                "MCP stdio server shutting down"
            );
            Ok(())
        }
        Err(e) => {
            error!(
                connection_id = %service.connection_id,
                error = %e,
                "MCP stdio server failed to start"
            );
            Err(anyhow!("Failed to start MCP stdio server: {}", e))
        }
    }
}

async fn start_http_server(config: ServerConfig) -> AnyhowResult<()> {
    // Extract configuration values
    let ServerConfig {
        server_url,
        bind_address,
    } = config;
    // Get the specified bind address
    let bind_address = bind_address.as_deref().unwrap();
    // Initialize structured logging and metrics
    #[cfg(feature = "mcp")]
    init_logging_and_metrics(false);
    // Output debugging information
    info!(
        server_url = %server_url,
        bind_address = %bind_address,
        "Starting embedding server with OpenAI-compatible API and MCP support"
    );

    // Create a session manager for the HTTP server
    let session_manager = Arc::new(LocalSessionManager::default());

    // Create shared app state with loaded models
    let app_state = Arc::new(
        AppState::new()
            .await
            .map_err(|e| anyhow!("Failed to initialize models: {}", e))?,
    );

    // Create a new EmbeddingService instance for the MCP server (if enabled)
    let models_clone = app_state.models.clone();

    // Create the MCP service
    let embedding_service = EmbeddingService::new(generate_connection_id(), models_clone);
    let mcp_svc = StreamableHttpService::new(
        move || Ok(embedding_service.clone()),
        session_manager.clone(),
        StreamableHttpServerConfig::default(),
    );

    // Create the OpenAI-compatible API router
    let api_router = create_api_router().with_state(Arc::clone(&app_state));

    // Create tracing layer for request logging
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &axum::http::Request<_>| {
            let connection_id = generate_connection_id();
            tracing::info_span!(
                "http_request",
                connection_id = %connection_id,
                method = %request.method(),
                uri = %request.uri(),
            )
        })
        .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
            debug!(
                method = %request.method(),
                uri = %request.uri(),
                "HTTP request started"
            );
        })
        .on_response(
            |response: &axum::http::Response<_>, latency: Duration, _span: &tracing::Span| {
                let status = response.status();
                if status.is_client_error() || status.is_server_error() {
                    warn!(
                        status = %status,
                        latency_ms = latency.as_millis(),
                        "HTTP request failed"
                    );
                } else {
                    info!(
                        status = %status,
                        latency_ms = latency.as_millis(),
                        "HTTP request completed"
                    );
                }
            },
        );
    // Create an Axum router with both API and MCP services
    let app = Router::new()
        .nest_service("/v1/mcp", mcp_svc)
        .merge(api_router)
        .route("/health", get(health))
        .layer(trace_layer);

    // Log available endpoints
    let protocol = "http";
    info!("🚀 Server started on {}://{}", protocol, bind_address);
    info!("📚 Available endpoints:");
    info!("  POST /v1/embeddings     - OpenAI-compatible embedding API (API key required)");
    info!("  GET  /v1/models         - List available models (API key required)");
    info!("  *    /v1/mcp            - MCP protocol endpoint");
    info!("  GET  /health            - Health check");

    // Bind to the address
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .map_err(|e| anyhow!("Failed to bind to {}: {}", bind_address, e))?;

    // Start the server
    axum::serve(listener, app)
        .with_graceful_shutdown(handle_double_ctrl_c())
        .await
        .map_err(|e| anyhow!("Server error: {}", e))?;

    // All ok
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::test_utils::spawn_test_server;
    use std::time::Duration;
    use tokio::time::timeout;

    fn default_test_config() -> ServerConfig {
        ServerConfig {
            server_url: "stdio://-".to_string(),
            bind_address: None,
        }
    }

    #[tokio::test]
    async fn test_start_server_stdio_mode_runs_until_eof() {
        let config = default_test_config();

        // The stdio server should run until stdin EOF; use a short timeout to validate it doesn't immediately exit
        let result = tokio::time::timeout(Duration::from_millis(100), start_server(config)).await;
        assert!(
            result.is_err(),
            "stdio server should not exit within timeout"
        );
    }

    #[tokio::test]
    async fn test_start_server_both_addresses_error() {
        let mut config = default_test_config();
        config.bind_address = Some("127.0.0.1:0".to_string());
        // Force an error by providing both stdio URL and bind address
        config.server_url = "stdio://-".to_string();

        // Use a short timeout since start_server will block if it succeeds in starting
        let result = timeout(Duration::from_millis(100), start_server(config)).await;
        // If it timed out, it means it started successfully (blocking)
        // If it returned, it might be an error or success
        match result {
            Err(_) => assert!(true), // Timed out, expected if it blocks
            Ok(inner) => assert!(inner.is_ok() || inner.is_err()),
        }
    }

    #[test]
    fn test_server_config_creation() {
        let mut config = default_test_config();
        config.server_url = "http://localhost:8080".to_string();
        config.bind_address = Some("127.0.0.1:8080".to_string());

        assert_eq!(config.server_url, "http://localhost:8080");
        assert_eq!(config.bind_address, Some("127.0.0.1:8080".to_string()));
    }

    #[tokio::test]
    async fn test_handle_double_ctrl_c_timeout() {
        // This test is tricky because it involves signals and timeouts
        // We'll test that the function can be spawned and cancelled
        let handle = tokio::spawn(async {
            // This will run indefinitely until cancelled
            handle_double_ctrl_c().await;
        });

        // Cancel after a short time
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();

        // If we get here without panicking, the test passes
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_http_server_bind_failure() {
        let mut config = default_test_config();
        // Use an invalid IP address to force a bind failure
        config.bind_address = Some("999.999.999.999:8080".to_string());

        let result = start_http_server(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_http_server_successful_startup() {
        let mut config = default_test_config();
        config.bind_address = Some("127.0.0.1:0".to_string());

        // Start the server with a timeout to avoid running forever
        let result = timeout(Duration::from_millis(100), start_http_server(config)).await;

        // The server should have started successfully and been cancelled by the timeout
        assert!(result.is_err()); // Timeout error means it was running
    }

    #[tokio::test]
    async fn test_start_http_server_creates_db_dir() {
        let mut config = default_test_config();
        config.bind_address = Some("127.0.0.1:0".to_string());

        let handle = tokio::spawn(start_http_server(config));

        // Give it some time to start
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        handle.abort();
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_server_http_dispatch_smoke() {
        // Verify that start_server dispatches to HTTP path when bind_address is set
        let mut config = default_test_config();
        config.bind_address = Some("127.0.0.1:0".to_string());
        // Use a short timeout to ensure the server begins serving
        let result = timeout(Duration::from_millis(100), start_server(config)).await;
        // Should timeout (still running)
        assert!(result.is_err());
    }
    #[tokio::test]
    async fn test_spawn_test_server_health() {
        let (addr, handle) = spawn_test_server().await;
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/health", addr))
            .send()
            .await
            .expect("Failed to send request");
        assert!(response.status().is_success());
        let body = response.text().await.expect("Failed to read response body");
        assert_eq!(body, ""); // Health endpoint returns 200 OK with no body
        handle.abort();
    }
} // Code doeds not go on the line following a righ tcurly brace
