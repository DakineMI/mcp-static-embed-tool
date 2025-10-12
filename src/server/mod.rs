pub mod api;
pub mod api_keys;
pub mod errors;
pub mod http;
pub mod limit;
pub mod start;
pub mod start_simple;
pub mod state;

pub use start_simple::start_http_server;

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
use crate::cli::StartArgs;
use crate::server::state::AppState;

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
    
    let model_name = request.model
        .or(params.model)
        .unwrap_or_else(|| state.default_model.clone());
    
    let model = state.models.get(&model_name)
        .unwrap_or_else(|| state.models.get(&state.default_model).unwrap());
    
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

