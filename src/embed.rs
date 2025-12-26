use anyhow::{Result, anyhow};
use model2vec_rs::model::StaticModel;
use std::path::PathBuf;

/// A high-performance static text embedder using Model2Vec.
pub struct Embedder {
    model: StaticModel,
}

impl Embedder {
    /// Create a new Embedder instance.
    ///
    /// This will attempt to load the model from the local cache.
    /// If the model is a built-in alias (e.g., "potion-8M") and not found locally,
    /// it will try to download it from HuggingFace.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model (e.g. "potion-32M", "minishlab/potion-base-32M") or path.
    pub fn new(model_name: &str) -> Result<Self> {
        let model_path = resolve_model_path(model_name)?;
        
        let model = if model_path.exists() {
            StaticModel::from_pretrained(&model_path, None, None, None)
                .map_err(|e| anyhow!("Failed to load model from path: {}", e))?
        } else {
            // Try as HF ID directly
            let hf_id = resolve_hf_id(model_name);
            StaticModel::from_pretrained(hf_id, None, None, None)
                .map_err(|e| anyhow!("Failed to load model '{}' (tried local path and HF): {}", model_name, e))?
        };

        Ok(Self { model })
    }

    /// Generate embedding for a single text string.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.model.encode(&[text.to_string()])[0].clone()
    }

    /// Generate embeddings for a batch of texts.
    pub fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.model.encode(texts)
    }
}

fn resolve_model_path(model_name: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow!("Could not determine home directory"))?;
    
    Ok(PathBuf::from(home)
        .join(".static-embedding-tool")
        .join("models")
        .join(model_name))
}

fn resolve_hf_id(model_name: &str) -> &str {
    match model_name {
        "potion-8M" => "minishlab/potion-base-8M",
        "potion-32M" => "minishlab/potion-base-32M",
        other => other,
    }
}
