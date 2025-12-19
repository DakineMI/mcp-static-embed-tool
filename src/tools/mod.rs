//! Model Context Protocol (MCP) tools for the embedding server.
//!
//! This module implements MCP tool endpoints that expose embedding functionality
//! through the MCP interface alongside the HTTP API.
//!
//! ## Available Tools
//!
//! - **embed**: Generate embeddings for a single text input
//! - **batch_embed**: Process multiple texts in parallel
//! - **list_models**: Query available embedding models
//! - **load_model**: Dynamically load a model into memory
//!
//! ## Connection Management
//!
//! Each MCP client session maintains:
//! - Unique connection ID
//! - Session start time
//! - Request metrics (count, last access)
//! - Lock-based state for thread safety
//!
//! ## Examples
//!
//! ```json
//! // MCP tool request: embed
//! {
//!   "input": "Hello world",
//!   "model": "potion-32M"
//! }
//!
//! // MCP tool request: batch_embed
//! {
//!   "inputs": ["Hello", "World"],
//!   "model": "potion-32M"
//! }
//! ```

use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content, Tool, ListToolsResult},
    handler::server::ServerHandler,
    service::RequestContext,
    RoleServer,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use metrics::counter;
use crate::utils;

// Global metrics
static EMBEDDING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Parameters for the embed tool.
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmbedParams {
    #[schemars(description = "Text input to generate embeddings for")]
    pub input: String,
    #[schemars(description = "Model to use for embedding (optional, defaults to potion-32M)")]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct BatchEmbedParams {
    #[schemars(description = "Array of text inputs to generate embeddings for")]
    pub inputs: Vec<String>,
    #[schemars(description = "Model to use for embedding (optional, defaults to potion-32M)")]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelListParams {}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelInfoParams {
    #[schemars(description = "Name of the model to get information about")]
    pub model: String,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelDistillParams {
    #[schemars(description = "Input model name or path")]
    pub input_model: String,
    #[schemars(description = "Output model name")]
    pub output_name: String,
    #[schemars(description = "Number of dimensions for PCA compression (optional, defaults to 128)")]
    pub dimensions: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
    pub model: String,
    pub dimensions: usize,
    pub processing_time_ms: u64,
}

#[derive(Serialize, Deserialize)]
pub struct BatchEmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub dimensions: usize,
    pub processing_time_ms: u64,
    pub input_count: usize,
}

#[derive(Clone)]
pub struct EmbeddingService {
    /// Connection ID for tracking this client session
    pub connection_id: String,
    /// Available Model2Vec models
    pub models: Arc<Mutex<HashMap<String, model2vec_rs::model::StaticModel>>>,
    /// Timestamp when this service was created
    pub created_at: std::time::Instant,

}

impl EmbeddingService {
    /// Create a new EmbeddingService instance
    pub fn new(connection_id: String) -> Self {
        info!(connection_id = %connection_id, "Creating new embedding service session");
        Self {
            connection_id,
            models: Arc::new(Mutex::new(HashMap::new())),
            created_at: Instant::now(),
        }
    }

    /// Generate embeddings for a single text input
    pub async fn embed(&self, params: EmbedParams) -> Result<CallToolResult, McpError> {
        let EmbedParams { input, model } = params;
        let start_time = Instant::now();

        counter!("embedtool.tools.embed").increment(1);
        let embedding_id = EMBEDDING_COUNTER.fetch_add(1, Ordering::SeqCst);

        debug!(
            connection_id = %self.connection_id,
            embedding_id = embedding_id,
            model = model.as_deref().unwrap_or("potion-32M"),
            input_length = input.len(),
            "Generating embedding for text input"
        );

        let model_name = model.unwrap_or_else(|| "potion-32M".to_string());
        let models_guard = self.models.lock().await;

        let model_instance = models_guard.get(&model_name)
            .ok_or_else(|| {
                error!(
                    connection_id = %self.connection_id,
                    model = %model_name,
                    "Model not found or not loaded"
                );
                counter!("embedtool.errors.model_not_found").increment(1);
                McpError::internal_error(
                    format!("Model '{}' not found. Available models: {:?}",
                           model_name,
                           models_guard.keys().collect::<Vec<_>>()),
                    None
                )
            })?;

        let embeddings = model_instance.encode(&[input.clone()]);
        if let Some(embedding) = embeddings.first() {
            let duration = start_time.elapsed();
            let dimensions = embedding.len();

                    let response = EmbeddingResponse {
                        embedding: embedding.clone(),
                        model: model_name.clone(),
                        dimensions,
                        processing_time_ms: duration.as_millis() as u64,
                    };

            info!(
                connection_id = %self.connection_id,
                embedding_id = embedding_id,
                model = %model_name,
                dimensions = dimensions,
                duration_ms = duration.as_millis(),
                "Successfully generated embedding"
            );

            let json_response = serde_json::to_string_pretty(&response)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;

            Ok(CallToolResult::success(vec![Content::text(json_response)]))
        } else {
            Err(McpError::internal_error("No embedding generated".to_string(), None))
        }
    }

    /// Generate embeddings for multiple text inputs in batch
    pub async fn batch_embed(&self, params: BatchEmbedParams) -> Result<CallToolResult, McpError> {
        let BatchEmbedParams { inputs, model } = params;
        let start_time = Instant::now();
        
        counter!("embedtool.tools.batch_embed").increment(1);
        let embedding_id = EMBEDDING_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        debug!(
            connection_id = %self.connection_id,
            embedding_id = embedding_id,
            model = model.as_deref().unwrap_or("potion-32M"),
            input_count = inputs.len(),
            "Generating batch embeddings"
        );

        let model_name = model.unwrap_or_else(|| "potion-32M".to_string());
        let models_guard = self.models.lock().await;
        
        let model_instance = models_guard.get(&model_name)
            .ok_or_else(|| {
                error!(
                    connection_id = %self.connection_id,
                    model = %model_name,
                    "Model not found or not loaded"
                );
                counter!("embedtool.errors.model_not_found").increment(1);
                McpError::internal_error(
                    format!("Model '{}' not found. Available models: {:?}", 
                           model_name, 
                           models_guard.keys().collect::<Vec<_>>()),
                    None
                )
            })?;

        let batch_embeddings = model_instance.encode(&inputs);
        let duration = start_time.elapsed();
        let dimensions = batch_embeddings.first().map(|e| e.len()).unwrap_or(0);

        let response = BatchEmbeddingResponse {
            embeddings: batch_embeddings,
            model: model_name.clone(),
            dimensions,
            processing_time_ms: duration.as_millis() as u64,
            input_count: inputs.len(),
        };

        info!(
            connection_id = %self.connection_id,
            embedding_id = embedding_id,
            model = %model_name,
            input_count = inputs.len(),
            dimensions = dimensions,
            duration_ms = duration.as_millis(),
            "Successfully generated batch embeddings"
        );

        let json_response = serde_json::to_string_pretty(&response)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json_response)]))
    }

    /// List available embedding models
    pub async fn list_models(&self, _params: ModelListParams) -> Result<CallToolResult, McpError> {
        let start_time = Instant::now();
        
        counter!("embedtool.tools.list_models").increment(1);
        
        debug!(
            connection_id = %self.connection_id,
            "Listing available models"
        );

        let models_guard = self.models.lock().await;
        
        let mut models_info = Vec::new();
        for (name, model) in models_guard.iter() {
            let embeddings = model.encode(&["test".to_string()]);
            let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);

            models_info.push(serde_json::json!({
                "name": name,
                "dimensions": dimensions,
                "type": "Model2Vec",
                "status": "loaded"
            }));
        }

        let duration = start_time.elapsed();
        
        let result = serde_json::json!({
            "models": models_info,
            "count": models_info.len(),
            "query_time_ms": duration.as_millis()
        });

        info!(
            connection_id = %self.connection_id,
            model_count = models_info.len(),
            duration_ms = duration.as_millis(),
            "Successfully listed available models"
        );

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    /// Get detailed information about a specific model
    pub async fn model_info(&self, params: ModelInfoParams) -> Result<CallToolResult, McpError> {
        let ModelInfoParams { model: model_name } = params;
        let start_time = Instant::now();
        
        counter!("embedtool.tools.model_info").increment(1);
        
        debug!(
            connection_id = %self.connection_id,
            model = %model_name,
            "Getting model information"
        );

        let models_guard = self.models.lock().await;
        
        let model_instance = models_guard.get(&model_name)
            .ok_or_else(|| {
                error!(
                    connection_id = %self.connection_id,
                    model = %model_name,
                    "Model not found"
                );
                counter!("embedtool.errors.model_not_found").increment(1);
                McpError::internal_error(
                    format!("Model '{}' not found. Available models: {:?}", 
                           model_name, 
                           models_guard.keys().collect::<Vec<_>>()),
                    None
                )
            })?;

        let embeddings = model_instance.encode(&["test".to_string()]);
        let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);

        let duration = start_time.elapsed();
        
        let result = serde_json::json!({
            "name": model_name,
            "dimensions": dimensions,
            "type": "Model2Vec",
            "status": "loaded",
            "capabilities": [
                "text_embedding",
                "semantic_search",
                "similarity_comparison"
            ],
            "query_time_ms": duration.as_millis()
        });

        info!(
            connection_id = %self.connection_id,
            model = %model_name,
            dimensions = dimensions,
            duration_ms = duration.as_millis(),
            "Successfully retrieved model information"
        );

        Ok(CallToolResult::success(vec![Content::text(
            result.to_string(),
        )]))
    }

    /// Distill a new Model2Vec model from an existing model
    pub async fn distill_model(&self, params: ModelDistillParams) -> Result<CallToolResult, McpError> {
        let ModelDistillParams { 
            input_model, 
            output_name, 
            dimensions 
        } = params;
        let start_time = Instant::now();
        
        counter!("embedtool.tools.distill_model").increment(1);
        
        let dims = dimensions.unwrap_or(128);
        
        info!(
            connection_id = %self.connection_id,
            input_model = %input_model,
            output_name = %output_name,
            dimensions = dims,
            "Starting model distillation process"
        );

        match utils::distill(&input_model, dims, None).await {
            Ok(output_path) => {
                let duration = start_time.elapsed();
                
                info!(
                    connection_id = %self.connection_id,
                    input_model = %input_model,
                    output_name = %output_name,
                    output_path = ?output_path,
                    dimensions = dims,
                    duration_ms = duration.as_millis(),
                    "Successfully distilled model"
                );

                match model2vec_rs::model::StaticModel::from_pretrained(&output_path, None, None, None) {
                    Ok(model) => {
                        let mut models_guard = self.models.lock().await;
                        models_guard.insert(output_name.clone(), model);
                        
                        info!(
                            connection_id = %self.connection_id,
                            model_name = %output_name,
                            "Successfully loaded distilled model into service"
                        );
                    }
                    Err(e) => {
                        warn!(
                            connection_id = %self.connection_id,
                            model_name = %output_name,
                            error = %e,
                            "Model distilled successfully but failed to load into service"
                        );
                    }
                }

                let result = serde_json::json!({
                    "message": "Model distillation completed successfully",
                    "input_model": input_model,
                    "output_name": output_name,
                    "output_path": output_path,
                    "dimensions": dims,
                    "processing_time_ms": duration.as_millis()
                });

                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
            Err(e) => {
                let duration = start_time.elapsed();
                
                error!(
                    connection_id = %self.connection_id,
                    input_model = %input_model,
                    output_name = %output_name,
                    dimensions = dims,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "Failed to distill model"
                );
                
                counter!("embedtool.errors.distill_model").increment(1);
                
                Err(McpError::internal_error(
                    format!("Failed to distill model '{}': {}", input_model, e),
                    None,
                ))
            }
        }
    }

    /// Add a model to the service by loading it from a file path
    pub async fn load_model(&self, name: &str, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            connection_id = %self.connection_id,
            model_name = %name,
            model_path = %path,
            "Loading model into service"
        );

        let model = model2vec_rs::model::StaticModel::from_pretrained(path, None, None, None)?;
        
        let mut models_guard = self.models.lock().await;
        models_guard.insert(name.to_string(), model);
        
        info!(
            connection_id = %self.connection_id,
            model_name = %name,
            "Successfully loaded model into service"
        );

        Ok(())
    }
}

// TODO: Fix ServerHandler trait implementation - disabled for now
impl ServerHandler for EmbeddingService {
    async fn list_tools(&self, _pagination: Option<rmcp::model::PaginatedRequestParam>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let tools = vec![
            Tool {
                name: "embed".into(),
                description: Some(r#"
                Generate embeddings for a single text input using Model2Vec.

                This function generates vector embeddings for the provided text using the specified
                Model2Vec model. The embeddings can be used for semantic search, similarity comparison,
                clustering, and other machine learning tasks.

                Available models include:
                - potion-8M: Lightweight model with 8M parameters
                - potion-32M: Balanced model with 32M parameters (default)
                - code-distilled: Specialized model for code embeddings
                "#.into()),
                input_schema: Arc::new(serde_json::from_value(serde_json::to_value(schemars::schema_for!(EmbedParams)).unwrap()).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "batch_embed".into(),
                description: Some(r#"
                Generate embeddings for multiple text inputs in batch using Model2Vec.

                This function generates vector embeddings for an array of text inputs using the
                specified Model2Vec model. This is more efficient than calling embed multiple times
                for processing multiple texts.

                The batch processing maintains the order of inputs, so the returned embeddings array
                corresponds to the input array by index.

                Examples:
                - batch_embed(["Hello world", "Goodbye world"])  # Uses default potion-32M model
                - batch_embed(["Hello", "World"], Some("potion-8M"))  # Uses specific model
                - batch_embed(["def hello():", "class World:"], Some("code-distilled"))  # Code embeddings
                "#.into()),
                input_schema: Arc::new(serde_json::from_value(serde_json::to_value(schemars::schema_for!(BatchEmbedParams)).unwrap()).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "list_models".into(),
                description: Some(r#"
                List available embedding models.

                This function returns information about all available Model2Vec models that can be
                used for generating embeddings. Each model has different characteristics in terms
                of size, performance, and specialization.

                The response includes model names, dimensions, and other metadata to help you choose
                the right model for your use case.
                "#.into()),
                input_schema: Arc::new(serde_json::from_value(serde_json::to_value(schemars::schema_for!(ModelListParams)).unwrap()).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "model_info".into(),
                description: Some(r#"
                Get detailed information about a specific embedding model.

                This function returns detailed information about a specific Model2Vec model, including
                its dimensions, capabilities, and current status.

                Examples:
                - model_info("potion-32M")  # Get info about the default model
                - model_info("code-distilled")  # Get info about the code model
                "#.into()),
                input_schema: Arc::new(serde_json::from_value(serde_json::to_value(schemars::schema_for!(ModelInfoParams)).unwrap()).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
            Tool {
                name: "distill_model".into(),
                description: Some(r#"
                Distill a new Model2Vec model from an existing model using PCA compression.

                This function creates a new compressed Model2Vec model using PCA dimensionality
                reduction. This is useful for creating smaller, faster models for deployment while
                maintaining most of the semantic quality.

                The distillation process:
                1. Loads the source model
                2. Applies PCA to reduce dimensions
                3. Saves the new model with the specified name
                4. Loads the new model into the service

                Examples:
                - distill_model("sentence-transformers/all-MiniLM-L6-v2", "my-mini-model")  # Default 128 dims
                - distill_model("microsoft/codebert-base", "code-128", Some(128))  # Custom dimensions
                - distill_model("potion-32M", "potion-64", Some(64))  # Compress existing model
                "#.into()),
                input_schema: Arc::new(serde_json::from_value(serde_json::to_value(schemars::schema_for!(ModelDistillParams)).unwrap()).unwrap()),
                output_schema: None,
                annotations: None,
                icons: None,
                title: None,
            },
        ];

        Ok(ListToolsResult { tools, next_cursor: None })
    }

    async fn call_tool(&self, request: rmcp::model::CallToolRequestParam, _context: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let args = request.arguments
            .ok_or_else(|| McpError::invalid_params("Missing arguments", None))?;

        match &request.name as &str {
            "embed" => {
                let params: EmbedParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                self.embed(params).await
            }
            "batch_embed" => {
                let params: BatchEmbedParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                self.batch_embed(params).await
            }
            "list_models" => {
                let params: ModelListParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                self.list_models(params).await
            }
            "model_info" => {
                let params: ModelInfoParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                self.model_info(params).await
            }
            "distill_model" => {
                let params: ModelDistillParams = serde_json::from_value(serde_json::Value::Object(args))
                    .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
                self.distill_model(params).await
            }
            _ => Err(McpError::invalid_params(
                format!("Unknown tool: {}", request.name),
                None,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_service_creation() {
        let connection_id = "test-conn-123".to_string();
        let service = EmbeddingService::new(connection_id.clone());
        
        assert_eq!(service.connection_id, connection_id);
        assert!(service.models.try_lock().is_ok());
        assert!(service.created_at.elapsed() < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_initialize_connection() {
        let service = EmbeddingService::new("test-conn".to_string());
        let result = service.initialize_connection().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_embed_params_serialization() {
        let params = EmbedParams {
            input: "Hello world".to_string(),
            model: Some("potion-32M".to_string()),
        };
        
        // Test that it can be serialized to JSON
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Hello world"));
        assert!(json.contains("potion-32M"));
        
        // Test deserialization
        let deserialized: EmbedParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.input, "Hello world");
        assert_eq!(deserialized.model, Some("potion-32M".to_string()));
    }

    #[test]
    fn test_batch_embed_params_serialization() {
        let params = BatchEmbedParams {
            inputs: vec!["Hello".to_string(), "world".to_string()],
            model: None,
        };
        
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("world"));
        
        let deserialized: BatchEmbedParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.inputs.len(), 2);
        assert_eq!(deserialized.model, None);
    }

    #[test]
    fn test_model_list_params() {
        let params = ModelListParams {};
        let json = serde_json::to_string(&params).unwrap();
        // Should serialize to empty object
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_model_info_params() {
        let params = ModelInfoParams {
            model: "potion-32M".to_string(),
        };
        
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("potion-32M"));
        
        let deserialized: ModelInfoParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, "potion-32M");
    }

    #[test]
    fn test_model_distill_params() {
        let params = ModelDistillParams {
            input_model: "large-model".to_string(),
            output_name: "distilled-model".to_string(),
            dimensions: Some(128),
        };
        
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("large-model"));
        assert!(json.contains("distilled-model"));
        assert!(json.contains("128"));
        
        let deserialized: ModelDistillParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.input_model, "large-model");
        assert_eq!(deserialized.output_name, "distilled-model");
        assert_eq!(deserialized.dimensions, Some(128));
    }

    #[test]
    fn test_model_distill_params_defaults() {
        let params = ModelDistillParams {
            input_model: "input".to_string(),
            output_name: "output".to_string(),
            dimensions: None,
        };
        
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: ModelDistillParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.dimensions, None);
    }

    #[test]
    fn test_embedding_response_structure() {
        let response = EmbeddingResponse {
            embedding: vec![0.1, 0.2, 0.3],
            model: "test-model".to_string(),
            dimensions: 3,
            processing_time_ms: 150,
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test-model"));
        assert!(json.contains("150"));
        assert!(json.contains("0.1"));
        
        let deserialized: EmbeddingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.embedding.len(), 3);
        assert_eq!(deserialized.model, "test-model");
        assert_eq!(deserialized.dimensions, 3);
        assert_eq!(deserialized.processing_time_ms, 150);
    }

    #[test]
    fn test_batch_embedding_response_structure() {
        let response = BatchEmbeddingResponse {
            embeddings: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            model: "batch-model".to_string(),
            dimensions: 2,
            processing_time_ms: 200,
            input_count: 2,
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("batch-model"));
        assert!(json.contains("200"));
        assert!(json.contains("2"));
        
        let deserialized: BatchEmbeddingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.embeddings.len(), 2);
        assert_eq!(deserialized.input_count, 2);
        assert_eq!(deserialized.processing_time_ms, 200);
    }

    #[test]
    fn test_embed_params_default_model() {
        let params = EmbedParams {
            input: "test".to_string(),
            model: None,
        };
        
        assert!(params.model.is_none());
        assert_eq!(params.input, "test");
    }

    #[test]
    fn test_batch_embed_params_empty() {
        let params = BatchEmbedParams {
            inputs: vec![],
            model: None,
        };
        
        assert_eq!(params.inputs.len(), 0);
    }

    #[test]
    fn test_model_info_params_construction() {
        let model_name = "test-model".to_string();
        let params = ModelInfoParams {
            model: model_name.clone(),
        };
        
        assert_eq!(params.model, model_name);
    }

    #[test]
    fn test_model_distill_params_with_dims() {
        let params = ModelDistillParams {
            input_model: "in".to_string(),
            output_name: "out".to_string(),
            dimensions: Some(256),
        };
        
        assert_eq!(params.dimensions, Some(256));
    }

    #[test]
    fn test_embedding_response_zero_time() {
        let response = EmbeddingResponse {
            embedding: vec![],
            model: "test".to_string(),
            dimensions: 0,
            processing_time_ms: 0,
        };
        
        assert_eq!(response.processing_time_ms, 0);
        assert_eq!(response.embedding.len(), 0);
    }

    #[test]
    fn test_batch_embedding_response_zero_inputs() {
        let response = BatchEmbeddingResponse {
            embeddings: vec![],
            model: "test".to_string(),
            dimensions: 0,
            processing_time_ms: 0,
            input_count: 0,
        };
        
        assert_eq!(response.input_count, 0);
        assert_eq!(response.embeddings.len(), 0);
    }

    #[test]
    fn test_embedding_service_models_lock() {
        let service = EmbeddingService::new("test-lock".to_string());
        
        // Test that we can acquire lock
        {
            let _lock = service.models.try_lock();
            assert!(_lock.is_ok());
        }
        
        // Test that we can acquire again after release
        {
            let _lock = service.models.try_lock();
            assert!(_lock.is_ok());
        }
    }

    #[test]
    fn test_embedding_service_created_at() {
        let service = EmbeddingService::new("test-time".to_string());
        let elapsed = service.created_at.elapsed();
        
        // Should be very recent
        assert!(elapsed < std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_load_model_nonexistent_path() {
        // Attempt to load a model from a non-existent path should return an error
        let service = EmbeddingService::new("test-load".to_string());
        let result = service.load_model("missing", "/path/that/does/not/exist").await;
        assert!(result.is_err());
    }
}
