use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
    Router,
};
use model2vec_rs::model::StaticModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};

/// Shared application state containing loaded models
#[derive(Clone)]
pub struct AppState {
    pub models: HashMap<String, StaticModel>,
    pub default_model: String,
    pub startup_time: SystemTime,
}

impl AppState {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut models = HashMap::new();
        
        // Load multiple models for flexibility
        info!("Loading Model2Vec models...");
        
        // Try loading potion-8M
        match StaticModel::from_pretrained("minishlab/potion-base-8M", None, None, None) {
            Ok(model) => {
                info!("✓ Loaded potion-8M model");
                models.insert("potion-8M".to_string(), model);
            }
            Err(e) => error!("✗ Failed to load potion-8M: {}", e),
        }
        
        // Try loading potion-32M
        match StaticModel::from_pretrained("minishlab/potion-base-32M", None, None, None) {
            Ok(model) => {
                info!("✓ Loaded potion-32M model");
                models.insert("potion-32M".to_string(), model);
            }
            Err(e) => error!("✗ Failed to load potion-32M: {}", e),
        }
        
        // Try loading custom distilled models if available
        if let Ok(code_model) = StaticModel::from_pretrained("./code-model-distilled", None, None, None) {
            info!("✓ Loaded custom code-distilled model");
            models.insert("code-distilled".to_string(), code_model);
        }
        
        if models.is_empty() {
            return Err("No models could be loaded".into());
        }
        
        let default_model = if models.contains_key("potion-32M") {
            "potion-32M".to_string()
        } else {
            models.keys().next().unwrap().clone()
        };
        
        info!("Loaded {} models, default: {}", models.len(), default_model);
        
        Ok(AppState {
            models,
            default_model,
            startup_time: SystemTime::now(),
        })
    }
}

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
) -> Result<ResponseJson<EmbeddingResponse>, StatusCode> {
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
                None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    };
    
    // Generate embeddings
    let embeddings = match model.encode(&request.input) {
        Ok(embeddings) => embeddings,
        Err(e) => {
            error!("Failed to generate embeddings: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
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

    // Calculate token usage (rough approximation)
    let total_tokens = request.input.iter()
        .map(|s| s.split_whitespace().count())
        .sum();

    let response = EmbeddingResponse {
        object: "list".to_string(),
        data,
        model: model_name,
        usage: Usage {
            prompt_tokens: total_tokens,
            total_tokens,
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