use axum::{
    extract::{Json, Query},
    response::Json as ResponseJson,
    routing::post,
    Router,
};
use model2vec_rs::model::StaticModel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;

mod cli;
mod server;
mod utils;
mod logs;
// Temporarily comment out SurrealDB-specific modules while focusing on CLI
// mod resources;
// mod tools;

#[derive(Deserialize)]
struct EmbeddingRequest {
    input: Vec<String>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct QueryParams {
    model: Option<String>,
}

#[derive(Serialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    model: String,
    usage: Usage,
}

#[derive(Serialize)]
struct EmbeddingData {
    object: String,
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: usize,
    total_tokens: usize,
}

struct AppState {
    models: HashMap<String, StaticModel>,
    default_model: String,
}

impl AppState {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut models = HashMap::new();
        
        // Load multiple models for flexibility
        models.insert(
            "potion-8M".to_string(),
            StaticModel::from_pretrained("minishlab/potion-base-8M", None, None, None)?
        );
        
        models.insert(
            "potion-32M".to_string(), 
            StaticModel::from_pretrained("minishlab/potion-base-32M", None, None, None)?
        );
        
        // Load custom distilled models if available
        if let Ok(code_model) = StaticModel::from_pretrained("./code-model-distilled", None, None, None) {
            models.insert("code-distilled".to_string(), code_model);
        }
        
        Ok(AppState {
            models,
            default_model: "potion-32M".to_string(),
        })
    }
}

async fn embeddings_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
    Json(request): Json<EmbeddingRequest>,
) -> ResponseJson<EmbeddingResponse> {
    
    let model_name = request.model
        .or(params.model)
        .unwrap_or_else(|| state.default_model.clone());
    
    let model = state.models.get(&model_name)
        .unwrap_or_else(|| state.models.get(&state.default_model).unwrap());
    
    let embeddings = model.encode(&request.input);
    
    let data = embeddings
        .into_iter()
        .enumerate()
        .map(|(index, embedding)| EmbeddingData {
            object: "embedding".to_string(),
            embedding,
            index,
        })
        .collect();

    let total_tokens = request.input.iter()
        .map(|s| s.split_whitespace().count())
        .sum();

    ResponseJson(EmbeddingResponse {
        data,
        model: model_name,
        usage: Usage {
            prompt_tokens: total_tokens,
            total_tokens,
        },
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run the CLI
    crate::cli::run_cli().await
}
