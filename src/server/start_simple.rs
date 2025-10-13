use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use axum::{Router, routing::get};
use tracing::info;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use crate::server::state::AppState;
use crate::server::embeddings_handler;
// use crate::server::limit::create_rate_limit_layer;

/// Start the embedding HTTP server
pub async fn start_http_server(
    bind_addr: &str,
    rate_limit_rps: u32,
    rate_limit_burst: u32,
) -> Result<()> {
    info!("Starting embedding HTTP server on {}", bind_addr);
    
    // Parse bind address
    let addr: SocketAddr = bind_addr.parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse bind address '{}': {}", bind_addr, e))?;

    // Initialize AppState with models
    let app_state = Arc::new(AppState::new().await.map_err(|e| anyhow::anyhow!("Failed to initialize app state: {}", e))?);

    // Create rate limiting layer
    // let rate_limit_layer = create_rate_limit_layer(rate_limit_rps, rate_limit_burst);

    // Build the router
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/embeddings", axum::routing::post(embeddings_handler))
        .with_state(app_state);

    // Add middleware layers
    app = app.layer(
        ServiceBuilder::new()
            .layer(CorsLayer::permissive())
            // .layer(rate_limit_layer)
    );

    // Create TCP listener
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind to address '{}': {}", addr, e))?;

    info!("Embedding HTTP server listening on {}", addr);

    // Start the server
    axum::serve(listener, app).await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}