// Copyright 2024 Dakine MI, Inc. or its affiliates. All Rights Reserved.
 


pub mod api;
pub mod errors;
pub mod http;
pub mod start;
pub mod start_simple;
pub mod state;

use crate::server::state::AppState;
use axum::{
    extract::{Json, Query},
    response::Json as ResponseJson,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct EmbeddingResponse {
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Usage,
}

#[derive(Serialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: usize,
}

#[derive(Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub total_tokens: usize,
}

pub async fn embeddings_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
    Json(request): Json<EmbeddingRequest>,
) -> ResponseJson<EmbeddingResponse> {
    let requested_model_name = request
        .model
        .or(params.model)
        .unwrap_or_else(|| state.default_model.clone());

    let (model_name, model) = if let Some(model) = state.models.get(&requested_model_name) {
        (requested_model_name, model)
    } else {
        (
            state.default_model.clone(),
            state.models.get(&state.default_model).unwrap(),
        )
    };

    let embeddings = model.encode(&request.input);

    let data = embeddings
        .iter()
        .enumerate()
        .map(|(index, embedding)| EmbeddingData {
            object: "embedding".to_string(),
            embedding: embedding.clone(),
            index,
        })
        .collect();

    ResponseJson(EmbeddingResponse {
        data,
        model: model_name,
        usage: Usage {
            prompt_tokens: request.input.iter().map(|s| s.len()).sum(),
            total_tokens: request.input.iter().map(|s| s.len()).sum(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Define a local mock model for testing with predictable outputs
    struct LocalMockModel;

    impl LocalMockModel {
        fn new() -> Self {
            Self
        }
    }

    impl crate::server::state::Model for LocalMockModel {
        fn encode(&self, inputs: &[String]) -> Vec<Vec<f32>> {
            inputs.iter().map(|_| vec![0.1, 0.2, 0.3]).collect()
        }
    }

    fn create_test_app_state() -> Arc<AppState> {
        let mut models: HashMap<String, Arc<dyn crate::server::state::Model>> = HashMap::new();
        models.insert(
            "potion-32M".to_string(),
            Arc::new(LocalMockModel::new()),
        );
        models.insert(
            "test-model".to_string(),
            Arc::new(LocalMockModel::new()),
        );

        Arc::new(AppState {
            models,
            default_model: "potion-32M".to_string(),
            startup_time: std::time::SystemTime::now(),
        })
    }

    #[tokio::test]
    async fn test_embeddings_handler_basic() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test text".to_string()],
            model: None,
        };

        let params = QueryParams { model: None };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.data[0].index, 0);
        assert_eq!(response.model, "potion-32M");
        assert_eq!(response.usage.prompt_tokens, 9); // "test text".len()
        assert_eq!(response.usage.total_tokens, 9);
    }

    #[tokio::test]
    async fn test_embeddings_handler_multiple_inputs() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["text 1".to_string(), "text 2".to_string()],
            model: Some("test-model".to_string()),
        };

        let params = QueryParams { model: None };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.model, "test-model");
        assert_eq!(response.usage.prompt_tokens, 12); // "text 1".len() + "text 2".len()
    }

    #[tokio::test]
    async fn test_embeddings_handler_model_from_query_params() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test".to_string()],
            model: None,
        };

        let params = QueryParams {
            model: Some("test-model".to_string()),
        };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.model, "test-model");
    }

    #[tokio::test]
    async fn test_embeddings_handler_request_model_overrides_query() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test".to_string()],
            model: Some("test-model".to_string()),
        };

        let params = QueryParams {
            model: Some("other-model".to_string()),
        };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.model, "test-model");
    }

    #[tokio::test]
    async fn test_embeddings_handler_empty_input() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec![],
            model: None,
        };

        let params = QueryParams { model: None };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.data.len(), 0);
        assert_eq!(response.usage.prompt_tokens, 0);
        assert_eq!(response.usage.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_embeddings_handler_fallback_to_default_model() {
        let mut models: HashMap<String, Arc<dyn crate::server::state::Model>> = HashMap::new();
        models.insert(
            "existing-model".to_string(),
            Arc::new(LocalMockModel::new()),
        );

        let state = Arc::new(AppState {
            models,
            default_model: "existing-model".to_string(),
            startup_time: std::time::SystemTime::now(),
        });

        let request = EmbeddingRequest {
            input: vec!["test".to_string()],
            model: Some("nonexistent-model".to_string()),
        };

        let params = QueryParams { model: None };

        let result =
            embeddings_handler(axum::extract::State(state), Query(params), Json(request)).await;

        let ResponseJson(response) = result;
        assert_eq!(response.model, "existing-model");
    }

    #[test]
    fn test_embedding_request_deserialization() {
        let json = r#"{
            "input": ["text1", "text2"],
            "model": "test-model"
        }"#;

        let request: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.input, vec!["text1", "text2"]);
        assert_eq!(request.model, Some("test-model".to_string()));
    }

    #[test]
    fn test_query_params_deserialization() {
        let json = r#"{"model": "query-model"}"#;

        let params: QueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.model, Some("query-model".to_string()));
    }

    #[test]
    fn test_embedding_response_serialization() {
        let response = EmbeddingResponse {
            data: vec![EmbeddingData {
                object: "embedding".to_string(),
                embedding: vec![0.1, 0.2, 0.3],
                index: 0,
            }],
            model: "test-model".to_string(),
            usage: Usage {
                prompt_tokens: 10,
                total_tokens: 10,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["model"], "test-model");
        assert_eq!(parsed["data"][0]["object"], "embedding");
        assert_eq!(parsed["data"][0]["index"], 0);
        assert_eq!(parsed["usage"]["prompt_tokens"], 10);
    }

    #[test]
    fn test_usage_calculation() {
        let usage = Usage {
            prompt_tokens: 100,
            total_tokens: 100,
        };

        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.total_tokens, 100);
    }

    #[test]
    fn test_embedding_data_structure() {
        let data = EmbeddingData {
            object: "embedding".to_string(),
            embedding: vec![1.0, 2.0, 3.0],
            index: 5,
        };

        assert_eq!(data.object, "embedding");
        assert_eq!(data.embedding, vec![1.0, 2.0, 3.0]);
        assert_eq!(data.index, 5);
    }

    #[tokio::test]
    async fn test_spawn_test_server_health() {
        use super::test_utils::spawn_test_server;
        use reqwest::Client;

        let (addr, handle) = spawn_test_server().await;

        let client = Client::new();
        let resp = client.get(format!("{}/health", addr)).send().await.unwrap();
        assert!(resp.status().is_success());

        // stop server
        handle.abort();
    }

    #[tokio::test]
    async fn test_spawn_test_server_embeddings_endpoint() {
        use super::test_utils::spawn_test_server;
        use reqwest::Client;
        use serde_json::json;

        let (addr, handle) = spawn_test_server().await;

        let client = Client::new();
        let url = format!("{}/v1/embeddings", addr);
        let payload = json!({
            "input": ["hello"],
            "model": "potion-32M"
        });

        let resp = client.post(&url).json(&payload).send().await.unwrap();
        assert!(resp.status().is_success());

        // stop server
        handle.abort();
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
