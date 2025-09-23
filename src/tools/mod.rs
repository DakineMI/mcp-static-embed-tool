use anyhow::Result;
use metrics::counter;
use rmcp::{
    ErrorData as McpError,
    handler::server::router::tool::ToolRouter,
    handler::server::tool::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::utils;

// Global metrics
static EMBEDDING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EmbedParams {
    #[schemars(description = "Text input to generate embeddings for")]
    pub input: String,
    #[schemars(description = "Model to use for embedding (optional, defaults to potion-32M)")]
    pub model: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct BatchEmbedParams {
    #[schemars(description = "Array of text inputs to generate embeddings for")]
    pub inputs: Vec<String>,
    #[schemars(description = "Model to use for embedding (optional, defaults to potion-32M)")]
    pub model: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ModelListParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ModelInfoParams {
    #[schemars(description = "Name of the model to get information about")]
    pub model: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
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
    pub models: Arc<Mutex<HashMap<String, model2vec_rs::StaticModel>>>,
    /// Timestamp when this service was created
    pub created_at: std::time::Instant,
    /// Router containing all available tools
    pub tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EmbeddingService {
    /// Create a new EmbeddingService instance
    pub fn new(connection_id: String) -> Self {
        info!(connection_id = %connection_id, "Creating new embedding service session");
        Self {
            connection_id,
            models: Arc::new(Mutex::new(HashMap::new())),
            created_at: Instant::now(),
            tool_router: Self::tool_router(),
        }
    }

    /// Generate embeddings for a single text input
    #[tool(description = r#"
Generate embeddings for a single text input using Model2Vec.

This function generates vector embeddings for the provided text using the specified 
Model2Vec model. The embeddings can be used for semantic search, similarity comparison, 
clustering, and other machine learning tasks.

Available models include:
- potion-8M: Lightweight model with 8M parameters
- potion-32M: Balanced model with 32M parameters (default)
- code-distilled: Specialized model for code embeddings

Examples:
- embed("Hello world")  # Uses default potion-32M model
- embed("Hello world", Some("potion-8M"))  # Uses specific model
- embed("def hello(): return 'world'", Some("code-distilled"))  # Code embedding
"#)]
    pub async fn embed(&self, params: Parameters<EmbedParams>) -> Result<CallToolResult, McpError> {
        let EmbedParams { input, model } = params.0;
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

        match model_instance.embed(&input) {
            Ok(embedding) => {
                let duration = start_time.elapsed();
                let dimensions = embedding.len();
                
                let response = EmbeddingResponse {
                    embedding,
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
            }
            Err(e) => {
                let duration = start_time.elapsed();
                
                error!(
                    connection_id = %self.connection_id,
                    embedding_id = embedding_id,
                    model = %model_name,
                    duration_ms = duration.as_millis(),
                    error = %e,
                    "Failed to generate embedding"
                );
                
                counter!("embedtool.errors.embed").increment(1);
                
                Err(McpError::internal_error(
                    format!("Failed to generate embedding with model '{}': {}", model_name, e),
                    None,
                ))
            }
        }
    }

    /// Generate embeddings for multiple text inputs in batch
    #[tool(description = r#"
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
"#)]
    pub async fn batch_embed(&self, params: Parameters<BatchEmbedParams>) -> Result<CallToolResult, McpError> {
        let BatchEmbedParams { inputs, model } = params.0;
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

        let mut embeddings = Vec::with_capacity(inputs.len());
        let mut dimensions = 0;
        
        for (idx, input) in inputs.iter().enumerate() {
            match model_instance.embed(input) {
                Ok(embedding) => {
                    if dimensions == 0 {
                        dimensions = embedding.len();
                    }
                    embeddings.push(embedding);
                }
                Err(e) => {
                    let duration = start_time.elapsed();
                    
                    error!(
                        connection_id = %self.connection_id,
                        embedding_id = embedding_id,
                        model = %model_name,
                        input_index = idx,
                        duration_ms = duration.as_millis(),
                        error = %e,
                        "Failed to generate embedding for input at index {}"
                    );
                    
                    counter!("embedtool.errors.batch_embed").increment(1);
                    
                    return Err(McpError::internal_error(
                        format!("Failed to generate embedding for input at index {}: {}", idx, e),
                        None,
                    ));
                }
            }
        }

        let duration = start_time.elapsed();
        
        let response = BatchEmbeddingResponse {
            embeddings,
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
    #[tool(description = r#"
List available embedding models.

This function returns information about all available Model2Vec models that can be 
used for generating embeddings. Each model has different characteristics in terms 
of size, performance, and specialization.

The response includes model names, dimensions, and other metadata to help you choose 
the right model for your use case.
"#)]
    pub async fn list_models(&self, _params: Parameters<ModelListParams>) -> Result<CallToolResult, McpError> {
        let start_time = Instant::now();
        
        counter!("embedtool.tools.list_models").increment(1);
        
        debug!(
            connection_id = %self.connection_id,
            "Listing available models"
        );

        let models_guard = self.models.lock().await;
        
        let mut models_info = Vec::new();
        for (name, model) in models_guard.iter() {
            let dimensions = match model.embed("test") {
                Ok(embedding) => embedding.len(),
                Err(_) => 0,
            };
            
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
    #[tool(description = r#"
Get detailed information about a specific embedding model.

This function returns detailed information about a specific Model2Vec model, including 
its dimensions, capabilities, and current status.

Examples:
- model_info("potion-32M")  # Get info about the default model
- model_info("code-distilled")  # Get info about the code model
"#)]
    pub async fn model_info(&self, params: Parameters<ModelInfoParams>) -> Result<CallToolResult, McpError> {
        let ModelInfoParams { model: model_name } = params.0;
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

        let dimensions = match model_instance.embed("test") {
            Ok(embedding) => embedding.len(),
            Err(e) => {
                error!(
                    connection_id = %self.connection_id,
                    model = %model_name,
                    error = %e,
                    "Failed to test model"
                );
                return Err(McpError::internal_error(
                    format!("Model '{}' appears to be corrupted: {}", model_name, e),
                    None
                ));
            }
        };

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
    #[tool(description = r#"
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
"#)]
    pub async fn distill_model(&self, params: Parameters<ModelDistillParams>) -> Result<CallToolResult, McpError> {
        let ModelDistillParams { 
            input_model, 
            output_name, 
            dimensions 
        } = params.0;
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

        match utils::distill(&input_model, &output_name, dims).await {
            Ok(output_path) => {
                let duration = start_time.elapsed();
                
                info!(
                    connection_id = %self.connection_id,
                    input_model = %input_model,
                    output_name = %output_name,
                    output_path = %output_path,
                    dimensions = dims,
                    duration_ms = duration.as_millis(),
                    "Successfully distilled model"
                );

                match model2vec_rs::StaticModel::from_file(&output_path) {
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

        let model = model2vec_rs::StaticModel::from_file(path)?;
        
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
