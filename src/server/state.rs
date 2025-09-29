use anyhow::{anyhow, Result};
use futures::future::join_all;
use model2vec_rs::model::StaticModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task;
use tracing::{error, info, warn};

/// Shared application state containing loaded models
#[derive(Clone)]
pub struct AppState {
    pub models: HashMap<String, StaticModel>,
    pub default_model: String,
    pub startup_time: SystemTime,
}

impl AppState {
    pub async fn new() -> Result<Self, anyhow::Error> {
        info!("Loading Model2Vec models...");

        let model_loads = vec![
            ("potion-8M".to_string(), "minishlab/potion-base-8M".to_string()),
            ("potion-32M".to_string(), "minishlab/potion-base-32M".to_string()),
            ("code-distilled".to_string(), "./code-model-distilled".to_string()),
        ];

        let handles: Vec<task::JoinHandle<Result<(String, StaticModel), anyhow::Error>>> = model_loads
            .into_iter()
            .map(|(name, path)| {
                let name = name.clone();
                let path = path.clone();
                let name_err = name.clone();
                task::spawn_blocking(move || {
                    StaticModel::from_pretrained(&path, None, None, None)
                        .map(|model| (name, model))
                        .map_err(|e| anyhow!(format!("Failed to load model {}: {}", name_err, e)))
                })
            })
            .collect();

        let results = join_all(handles).await;

        let mut models = HashMap::new();

        for result in results {
            match result {
                Ok(Ok((name, model))) => {
                    info!("✓ Loaded {} model", name);
                    models.insert(name, model);
                }
                Ok(Err(e)) => {
                    warn!("✗ {}", e);
                }
                Err(e) => {
                    warn!("✗ Failed to join model loading task: {}", e);
                }
            }
        }

        if models.is_empty() {
            return Err(anyhow!("No models could be loaded"));
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