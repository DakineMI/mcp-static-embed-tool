use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
    Router,
};
use anyhow::Result;
use model2vec_rs::model::StaticModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

/// Shared application state containing loaded models
#[derive(Clone)]
pub struct AppState {
    pub models: HashMap<String, StaticModel>,
    pub default_model: String,
    pub startup_time: SystemTime,
}

use super::state::AppState;

// ============================================================================
// Request/Response Structures
// ============================================================================

#[derive(Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    pub model: Option<String>,
    pub encoding_format: Option<String>, // "float" or "base64" (we only support float)
    pub dimensions: Option<usize>,       // For dimension reduction (not implemented)
    pub user: Option<String>,            // For tracking
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct EmbeddingResponse {
    pub object: String,
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

#[derive(Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

#[derive(Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: ErrorDetails,
}

#[derive(Serialize)]
pub struct ErrorDetails {
    pub message: String,
    pub r#type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// POST /v1/embeddings - OpenAI-compatible embedding endpoint
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
        match model.encode(&request.input) {
            Ok(embeddings) => embeddings,
            Err(e) => {
                error!("Failed to generate embeddings: {}", e);
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
    } else {
        // Large batch: split into chunks of 32 and process in parallel
        use futures::future::join_all;
        use tokio::task::spawn_blocking;

        let chunk_size = 32;
        let chunks: Vec<_> = request.input.chunks(chunk_size).collect();
        let mut chunk_futures = Vec::new();

        for chunk in chunks {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let model_clone = model.clone(); // Assuming StaticModel is Clone
            chunk_futures.push(spawn_blocking(move || model_clone.encode(&chunk_vec)));
        }

        let results = join_all(chunk_futures).await;
        let mut all_embeddings = Vec::new();

        for result in results {
            match result {
                Ok(Ok(embeddings)) => all_embeddings.extend(embeddings),
                Ok(Err(e)) => {
                    error!("Failed to generate chunk embeddings: {}", e);
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

    // Usage for embeddings is 0 tokens
    let response = EmbeddingResponse {
        object: "list".to_string(),
        data,
        model: model_name,
        usage: Usage {
            prompt_tokens: 0,
            total_tokens: 0,
        },
    };

    Ok(ResponseJson(response))
}

/// GET /v1/models - List available models
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

/// Handler for unsupported endpoints
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