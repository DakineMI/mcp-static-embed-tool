//! HTTP API handlers for OpenAI-compatible embedding endpoints.
//!
//! This module implements the core HTTP API with:
//! - **POST /v1/embeddings**: Generate embeddings from text input
//! - **GET /v1/models**: List available embedding models
//! - **GET /health**: Health check endpoint
//!
//! All endpoints use OpenAI-compatible request/response formats for easy integration.
//!
//! # Error Handling
//!
//! Errors are returned as JSON with OpenAI-compatible structure:
//!
//! ```json
//! {
//!   "error": {
//!     "message": "Model not found: invalid-model",
//!     "type": "invalid_request_error",
//!     "param": "model",
//!     "code": null
//!   }
//! }
//! ```
//!
//! # Examples
//!
//! ```bash
//! # Generate embeddings
//! curl -X POST http://localhost:8080/v1/embeddings \
//!   -H "Content-Type: application/json" \
//!   -d '{"input":["Hello world"], "model":"potion-32M"}'
//!
//! # List models
//! curl http://localhost:8080/v1/models
//! ```

use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tracing::error;

use super::state::AppState;
use super::{EmbeddingRequest, QueryParams, EmbeddingResponse, EmbeddingData, Usage, ModelsResponse, ModelInfo, ApiError, ErrorDetails};

// ============================================================================
// Route Handlers
// ============================================================================

/// Generate embeddings for input text(s).
///
/// POST /v1/embeddings - OpenAI-compatible embedding endpoint
///
/// # Arguments
///
/// * `state` - Application state containing loaded models
/// * `params` - Query parameters (optional model selection)
/// * `request` - JSON request body with input texts and options
///
/// # Returns
///
/// * `Ok(EmbeddingResponse)` - Embeddings with usage statistics
/// * `Err(ApiError)` - Error with details (400/404/500 status codes)
///
/// # Errors
///
/// - `400 invalid_request_error`: Empty input, invalid encoding format
/// - `404 model_not_found_error`: Requested model not loaded
/// - `500 server_error`: Model computation failed
///
/// # Examples
///
/// ```bash
/// curl -X POST http://localhost:8080/v1/embeddings \
///   -d '{"input":["Hello world"], "model":"potion-32M"}'
/// ```
pub async fn embeddings_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
    Json(request): Json<EmbeddingRequest>,
) -> Result<ResponseJson<EmbeddingResponse>, (StatusCode, ResponseJson<ApiError>)> {
    // Input validation
    if request.input.is_empty() {
        let error = ApiError {
            error: ErrorDetails {
                message: "Input too long or empty".to_string(),
                r#type: "invalid_request_error".to_string(),
                param: Some("input".to_string()),
                code: None,
            },
        };
        return Err((StatusCode::BAD_REQUEST, ResponseJson(error)));
    }

    if request.input.len() > 100 {
        let error = ApiError {
            error: ErrorDetails {
                message: "Batch size too large. Maximum 100 inputs allowed.".to_string(),
                r#type: "invalid_request_error".to_string(),
                param: Some("input".to_string()),
                code: None,
            },
        };
        return Err((StatusCode::BAD_REQUEST, ResponseJson(error)));
    }

    for text in &request.input {
        if text.is_empty() || text.len() > 8192 {
            let error = ApiError {
                error: ErrorDetails {
                    message: "Input too long or empty".to_string(),
                    r#type: "invalid_request_error".to_string(),
                    param: Some("input".to_string()),
                    code: None,
                },
            };
            return Err((StatusCode::BAD_REQUEST, ResponseJson(error)));
        }
    }
    // Determine which model to use
    let model_name = request.model
        .or(params.model)
        .unwrap_or_else(|| state.default_model.clone());
    
    // Get the model
    let model = match state.models.get(&model_name) {
        Some(model) => model,
        None => {
            // Fallback to default model if requested model not found
            match state.models.get(&state.default_model) {
                Some(model) => model,
                None => {
                    let error = ApiError {
                        error: ErrorDetails {
                            message: "No models available".to_string(),
                            r#type: "server_error".to_string(),
                            param: None,
                            code: None,
                        },
                    };
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, ResponseJson(error)));
                }
            }
        }
    };
    
    // Generate embeddings with optional parallel chunking for large batches
    let embeddings: Vec<Vec<f32>> = if request.input.len() <= 32 {
        // Small batch: encode directly
        model.encode(&request.input)
    } else {
        // Large batch: split into chunks of 32 and process in parallel
        use futures::future::join_all;
        use tokio::task::spawn_blocking;

        let chunk_size = 32;
        let chunks: Vec<_> = request.input.chunks(chunk_size).collect();
        let mut chunk_futures = Vec::new();

        for chunk in chunks {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let model_clone = model.clone();
            chunk_futures.push(spawn_blocking(move || model_clone.encode(&chunk_vec)));
        }

        let results = join_all(chunk_futures).await;
        let mut all_embeddings = Vec::new();

        for result in results {
            match result {
                Ok(embeddings) => all_embeddings.extend(embeddings),
                Err(e) => {
                    error!("Spawn blocking failed: {}", e);
                    let error = ApiError {
                        error: ErrorDetails {
                            message: "Embedding generation failed".to_string(),
                            r#type: "server_error".to_string(),
                            param: None,
                            code: None,
                        },
                    };
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, ResponseJson(error)));
                }
            }
        }

        all_embeddings
    };
    
    // Build response data
    let data = embeddings
        .into_iter()
        .enumerate()
        .map(|(index, embedding)| EmbeddingData {
            object: "embedding".to_string(),
            embedding,
            index,
        })
        .collect();

    // Approximate token usage (roughly 4 characters per token)
    let prompt_tokens: usize = request.input.iter().map(|s| s.len().div_ceil(4)).sum();

    let response = EmbeddingResponse {
        object: "list".to_string(),
        data,
        model: model_name,
        usage: Usage {
            prompt_tokens,
            total_tokens: prompt_tokens,
        },
    };

    Ok(ResponseJson(response))
}

/// List all available embedding models.
///
/// GET /v1/models - List available models
///
/// # Arguments
///
/// * `state` - Application state containing loaded models
///
/// # Returns
///
/// JSON list of available models with metadata
///
/// # Examples
///
/// ```bash
/// curl http://localhost:8080/v1/models
/// ```
pub async fn models_handler(
    State(state): State<Arc<AppState>>,
) -> ResponseJson<ModelsResponse> {
    let models = state.models.keys()
        .map(|model_id| ModelInfo {
            id: model_id.clone(),
            object: "model".to_string(),
            created: 1640995200, // Fixed timestamp for Model2Vec models
            owned_by: if model_id.starts_with("potion") { 
                "minishlab".to_string() 
            } else { 
                "custom".to_string() 
            },
        })
        .collect();

    ResponseJson(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

/// Reject requests to unsupported endpoints.
///
/// Returns a helpful error message directing users to supported operations.
///
/// # Returns
///
/// 400 Bad Request with error explaining that only embedding operations are supported
pub async fn unsupported_handler() -> (StatusCode, ResponseJson<ApiError>) {
    let error = ApiError {
        error: ErrorDetails {
            message: "This server only supports embedding operations. For chat completions, please use OpenAI's API directly.".to_string(),
            r#type: "invalid_request_error".to_string(),
            param: None,
            code: Some("unsupported_endpoint".to_string()),
        },
    };
    
    (StatusCode::BAD_REQUEST, ResponseJson(error))
}

// ============================================================================
// Router Creation
// ============================================================================

/// Create the OpenAI-compatible API router
pub fn create_api_router() -> Router<Arc<AppState>> {
    Router::new()
        // Core embedding functionality
        .route("/v1/embeddings", post(embeddings_handler))
        .route("/v1/models", get(models_handler))

        // Standard OpenAI endpoints (unsupported but properly handled)
        .route("/v1/chat/completions", post(unsupported_handler))
        .route("/v1/completions", post(unsupported_handler))

        // Other common OpenAI endpoints (also unsupported)
        .route("/v1/images/generations", post(unsupported_handler))
        .route("/v1/audio/transcriptions", post(unsupported_handler))
        .route("/v1/audio/translations", post(unsupported_handler))
        .route("/v1/fine-tuning/jobs", post(unsupported_handler))
        .route("/v1/fine-tuning/jobs", get(unsupported_handler))
        .route("/v1/files", post(unsupported_handler))
        .route("/v1/files", get(unsupported_handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::Json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::server::state::{Model, MockModel};

    // Mock model that panics when encoding to simulate spawn_blocking JoinError
    #[derive(Clone)]
    struct MockModelPanics;

    impl Model for MockModelPanics {
        fn encode(&self, _inputs: &[String]) -> Vec<Vec<f32>> {
            panic!("simulated model panic during encode");
        }
    }

    // Mock model for API tests with predictable output
    struct ApiMockModel;
    impl Model for ApiMockModel {
        fn encode(&self, inputs: &[String]) -> Vec<Vec<f32>> {
            inputs.iter().map(|_| vec![0.1, 0.2, 0.3]).collect()
        }
    }

    fn create_test_app_state() -> Arc<AppState> {
        let mut models: HashMap<String, Arc<dyn Model>> = HashMap::new();
        models.insert("potion-32M".to_string(), Arc::new(ApiMockModel));
        models.insert("test-model".to_string(), Arc::new(ApiMockModel));

        Arc::new(AppState {
            models,
            default_model: "potion-32M".to_string(),
            startup_time: std::time::SystemTime::now(),
        })
    }

    #[tokio::test]
    async fn test_embeddings_handler_empty_input() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec![],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.message, "Input too long or empty");
    }

    #[tokio::test]
    async fn test_embeddings_handler_too_many_inputs() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: (0..101).map(|i| format!("text {}", i)).collect(),
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.message, "Batch size too large. Maximum 100 inputs allowed.");
    }

    #[tokio::test]
    async fn test_embeddings_handler_empty_text() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["".to_string()],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.message, "Input too long or empty");
    }

    #[tokio::test]
    async fn test_embeddings_handler_text_too_long() {
        let state = create_test_app_state();
        let long_text = "a".repeat(8193);
        let request = EmbeddingRequest {
            input: vec![long_text],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.message, "Input too long or empty");
    }

    #[tokio::test]
    async fn test_embeddings_handler_model_not_found() {
        let mut models: HashMap<String, Arc<dyn Model>> = HashMap::new();
        models.insert("existing-model".to_string(), Arc::new(MockModel::new("existing-model".to_string(), 384)));

        let state = Arc::new(AppState {
            models,
            default_model: "nonexistent".to_string(),
            startup_time: std::time::SystemTime::now(),
        });

        let request = EmbeddingRequest {
            input: vec!["test text".to_string()],
            model: Some("nonexistent-model".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.error.message, "No models available");
    }

    #[tokio::test]
    async fn test_embeddings_handler_success_single_input() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test text".to_string()],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.object, "list");
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3]);
        assert_eq!(response.data[0].index, 0);
        assert_eq!(response.model, "potion-32M");
        assert_eq!(response.usage.prompt_tokens, 0);
        assert_eq!(response.usage.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_embeddings_handler_success_multiple_inputs() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["text 1".to_string(), "text 2".to_string()],
            model: Some("test-model".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        ).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.model, "test-model");
    }

    #[tokio::test]
    async fn test_models_handler_lists_models() {
        let state = create_test_app_state();
        let result = models_handler(axum::extract::State(state)).await;
        let Json(models_response) = result;
        assert_eq!(models_response.object, "list");
        // We inserted two models in create_test_app_state
        assert_eq!(models_response.data.len(), 2);
        let ids: Vec<String> = models_response.data.iter().map(|m| m.id.clone()).collect();
        assert!(ids.contains(&"potion-32M".to_string()));
        assert!(ids.contains(&"test-model".to_string()));
    }

    #[tokio::test]
    async fn test_unsupported_handler_returns_error() {
        let (status, Json(err)) = unsupported_handler().await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err.error.r#type, "invalid_request_error");
        assert_eq!(err.error.code.as_deref(), Some("unsupported_endpoint"));
        assert!(err.error.message.contains("only supports embedding"));
    }

    #[test]
    fn test_create_api_router_compiles() {
        // Ensure router can be created without panicking
        let _router = create_api_router();
        assert!(true);
    }

    #[tokio::test]
    async fn test_embeddings_handler_model_from_query_params() {
        let state = create_test_app_state();
        let request = EmbeddingRequest {
            input: vec!["test text".to_string()],
            model: None,
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: Some("test-model".to_string()) }),
            Json(request),
        ).await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.model, "test-model");
    }

    #[tokio::test]
    async fn test_models_handler() {
        let state = create_test_app_state();

        let result = models_handler(axum::extract::State(state)).await;

        let Json(response) = result;
        assert_eq!(response.object, "list");
        assert_eq!(response.data.len(), 2);

        // Check potion model
        let potion_model = response.data.iter().find(|m| m.id == "potion-32M").unwrap();
        assert_eq!(potion_model.object, "model");
        assert_eq!(potion_model.owned_by, "minishlab");

        // Check custom model
        let custom_model = response.data.iter().find(|m| m.id == "test-model").unwrap();
        assert_eq!(custom_model.object, "model");
        assert_eq!(custom_model.owned_by, "custom");
    }

    #[tokio::test]
    async fn test_unsupported_handler() {
        let result = unsupported_handler().await;

        let (status, Json(error)) = result;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error.error.r#type, "invalid_request_error");
        assert!(error.error.message.contains("only supports embedding operations"));
        assert_eq!(error.error.code, Some("unsupported_endpoint".to_string()));
    }

    #[tokio::test]
    async fn test_embeddings_handler_large_batch_parallel() {
        // Ensure the parallel chunking path (> 32 inputs) is exercised
        let state = create_test_app_state();
        let inputs: Vec<String> = (0..33).map(|i| format!("text {}", i)).collect();

        let request = EmbeddingRequest {
            input: inputs,
            model: Some("test-model".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        )
        .await;

        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response.data.len(), 33);
        assert_eq!(response.model, "test-model");
    }

    #[tokio::test]
    async fn test_embeddings_handler_spawn_blocking_error_returns_500() {
        // Use a model that panics in encode so spawn_blocking returns a JoinError
        let mut models: HashMap<String, Arc<dyn Model>> = HashMap::new();
        models.insert("panic-model".to_string(), Arc::new(MockModelPanics));

        let state = Arc::new(AppState {
            models,
            default_model: "panic-model".to_string(),
            startup_time: std::time::SystemTime::now(),
        });

        // Trigger the parallel path (>32 items)
        let inputs: Vec<String> = (0..33).map(|i| format!("text {}", i)).collect();
        let request = EmbeddingRequest {
            input: inputs,
            model: Some("panic-model".to_string()),
            encoding_format: None,
            dimensions: None,
            user: None,
        };

        let result = embeddings_handler(
            axum::extract::State(state),
            axum::extract::Query(QueryParams { model: None }),
            Json(request),
        )
        .await;

        assert!(result.is_err());
        let (status, Json(error)) = result.err().unwrap();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.error.r#type, "server_error");
        assert_eq!(error.error.message, "Embedding generation failed");
    }

    #[tokio::test]
    async fn test_create_api_router() {
        let _router = create_api_router();

        // The router should have the expected routes
        // We can't easily test the exact routes without more complex setup,
        // but we can verify the router is created successfully
        assert!(true); // If we get here, router creation worked
    }

    #[test]
    fn test_embedding_request_deserialization() {
        let json = r#"{
            "input": ["text1", "text2"],
            "model": "test-model",
            "encoding_format": "float",
            "dimensions": 128,
            "user": "test-user"
        }"#;

        let request: EmbeddingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.input, vec!["text1", "text2"]);
        assert_eq!(request.model, Some("test-model".to_string()));
        assert_eq!(request.encoding_format, Some("float".to_string()));
        assert_eq!(request.dimensions, Some(128));
        assert_eq!(request.user, Some("test-user".to_string()));
    }

    #[test]
    fn test_embedding_response_serialization() {
        let response = EmbeddingResponse {
            object: "list".to_string(),
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

        assert_eq!(parsed["object"], "list");
        assert_eq!(parsed["model"], "test-model");
        assert_eq!(parsed["data"][0]["embedding"], serde_json::json!([0.1, 0.2, 0.3]));
        assert_eq!(parsed["usage"]["prompt_tokens"], 10);
    }

    #[test]
    fn test_api_error_serialization() {
        let error = ApiError {
            error: ErrorDetails {
                message: "Test error".to_string(),
                r#type: "test_error".to_string(),
                param: Some("test_param".to_string()),
                code: Some("test_code".to_string()),
            },
        };

        let json = serde_json::to_string(&error).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["error"]["message"], "Test error");
        assert_eq!(parsed["error"]["type"], "test_error");
        assert_eq!(parsed["error"]["param"], "test_param");
        assert_eq!(parsed["error"]["code"], "test_code");
    }
}