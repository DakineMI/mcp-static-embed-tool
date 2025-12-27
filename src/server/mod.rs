// Copyright 2024 Dakine MI, Inc. or its affiliates. All Rights Reserved.
 


pub mod api;
pub mod errors;
pub mod http;
pub mod start;
pub mod start_simple;
pub mod state;

pub mod logs;

use serde::{Deserialize, Serialize};

// ============================================================================
// Request/Response Structures (OpenAI-compatible)
// ============================================================================

/// Request structure for POST /v1/embeddings endpoint.
#[derive(Deserialize)]
pub struct EmbeddingRequest {
    /// Input text(s) to generate embeddings for. Cannot be empty.
    pub input: Vec<String>,
    /// Model to use for embedding generation. If omitted, uses default model.
    pub model: Option<String>,
    /// Encoding format for embeddings. Only "float" is supported.
    pub encoding_format: Option<String>,
    /// Target dimensions for output embeddings (not yet implemented).
    pub dimensions: Option<usize>,
    /// User identifier for tracking and analytics.
    pub user: Option<String>,
}

/// Query parameters for endpoints supporting model selection.
#[derive(Deserialize)]
pub struct QueryParams {
    /// Optional model name parameter for GET endpoints.
    pub model: Option<String>,
}

/// Response structure for POST /v1/embeddings endpoint.
#[derive(Serialize)]
pub struct EmbeddingResponse {
    /// Object type identifier ("list").
    pub object: String,
    /// Array of embedding results, one per input text.
    pub data: Vec<EmbeddingData>,
    /// Model used for generating embeddings.
    pub model: String,
    /// Token usage statistics.
    pub usage: Usage,
}

/// Individual embedding result within EmbeddingResponse.
#[derive(Serialize)]
pub struct EmbeddingData {
    /// Object type identifier ("embedding").
    pub object: String,
    /// Dense vector embedding.
    pub embedding: Vec<f32>,
    /// Index of this embedding in the input array.
    pub index: usize,
}

/// Token usage statistics for billing and monitoring.
#[derive(Serialize)]
pub struct Usage {
    /// Number of tokens processed (approximated from input length).
    pub prompt_tokens: usize,
    /// Total tokens (same as prompt_tokens for embeddings).
    pub total_tokens: usize,
}

/// Response structure for GET /v1/models endpoint.
#[derive(Serialize)]
pub struct ModelsResponse {
    /// Object type identifier ("list").
    pub object: String,
    /// Array of available models.
    pub data: Vec<ModelInfo>,
}

/// Information about a single available model.
#[derive(Serialize)]
pub struct ModelInfo {
    /// Model identifier used in API requests.
    pub id: String,
    /// Object type identifier ("model").
    pub object: String,
    /// Unix timestamp of model creation/loading.
    pub created: u64,
    /// Owner/provider of the model (e.g., "static-embedding-server").
    pub owned_by: String,
}

/// API error response structure (OpenAI-compatible).
#[derive(Serialize, Debug)]
pub struct ApiError {
    /// Error details.
    pub error: ErrorDetails,
}

/// Detailed error information.
#[derive(Serialize, Debug)]
pub struct ErrorDetails {
    /// Human-readable error message.
    pub message: String,
    /// Error classification ("invalid_request_error", "server_error", etc.).
    pub r#type: String,
    /// Parameter that caused the error (if applicable).
    pub param: Option<String>,
    /// Error code for programmatic handling.
    pub code: Option<String>,
}

/// Re-export the main handler from api.rs
pub use api::embeddings_handler;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use axum::extract::{Json, Query, State};
    use crate::server::api::embeddings_handler;
    use crate::server::state::AppState;

    // Define a local mock model for testing with predictable outputs
    struct LocalMockModel;

    impl crate::server::state::Model for LocalMockModel {
        fn encode(&self, inputs: &[String]) -> Vec<Vec<f32>> {
            inputs.iter().map(|_| vec![0.1, 0.2, 0.3]).collect()
        }
    }

    fn create_test_app_state() -> Arc<AppState> {
        let mut models: HashMap<String, Arc<dyn crate::server::state::Model>> = HashMap::new();
        models.insert(
            "potion-32M".to_string(),
            Arc::new(LocalMockModel),
        );

        Arc::new(AppState {
            models,
            default_model: "potion-32M".to_string(),
            startup_time: std::time::SystemTime::now(),
        })
    }

    #[tokio::test]
    async fn test_embeddings_handler_happy_path() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test text".to_string()],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let params = QueryParams { model: None };

        let result =
            embeddings_handler(State(state), Query(params), Json(request)).await;

        assert!(result.is_ok());
        let axum::response::Json(response) = result.unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.model, "potion-32M");
    }
}

#[cfg(test)]
pub mod test_utils {
    use crate::server::state::AppState;
    use axum::{Router, routing::{get, post}};
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;
    use tower_http::trace::TraceLayer;
    use tracing::{debug, info};
    use uuid::Uuid;

    pub async fn spawn_test_server() -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test listener");
        let addr = listener.local_addr().expect("Failed to get local addr");
        let addr_str = format!("http://{}", addr);

        // Use a unique secure temp directory for test databases
        let _tmp = tempfile::tempdir().expect("failed to create tempdir");
        let app_state = Arc::new(AppState::new().await.expect("Failed to create AppState"));

        let trace_layer = TraceLayer::new_for_http()
            .make_span_with(|request: &axum::http::Request<_>| {
                let connection_id = Uuid::new_v4().to_string();
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
                |response: &axum::http::Response<_>,
                 latency: std::time::Duration,
                 _span: &tracing::Span| {
                    let status = response.status();
                    if status.is_client_error() || status.is_server_error() {
                        info!(
                            status = %status,
                            latency_ms = latency.as_millis(),
                            "HTTP request failed"
                        );
                    } else {
                        debug!(
                            status = %status,
                            latency_ms = latency.as_millis(),
                            "HTTP request completed"
                        );
                    }
                },
            );

        let router = Router::new()
            .nest_service("/v1/mcp", Router::new()) // Skip MCP for tests
            .route("/health", get(crate::server::http::health))
            .route("/v1/embeddings", post(crate::server::embeddings_handler))
            .with_state(app_state)
            .layer(trace_layer);

        let server = axum::serve(listener, router);

        let handle = tokio::spawn(async move {
            let _ = server.await;
        });

        (addr_str, handle)
    }
}
