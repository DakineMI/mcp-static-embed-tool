use crate::cli::{ModelAction, DownloadArgs, DistillArgs, RemoveArgs, UpdateArgs, InfoArgs};
use anyhow::Result as AnyhowResult;
use std::path::PathBuf;
use std::fs;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ModelInfo {
    name: String,
    path: String,
    source: String, // "huggingface", "local", "distilled"
    dimensions: Option<usize>,
    size_mb: Option<f64>,
    downloaded_at: String,
    description: Option<String>,
}

pub async fn handle_model_command(
    action: ModelAction,
    _config_path: Option<PathBuf>,
) -> AnyhowResult<()> {
    match action {
        ModelAction::List => list_models().await,
        ModelAction::Download(args) => download_model(args).await,
        ModelAction::Distill(args) => distill_model(args).await,
        ModelAction::Remove(args) => remove_model(args).await,
        ModelAction::Update(args) => update_model(args).await,
        ModelAction::Info(args) => show_model_info(args).await,
    }
}

async fn list_models() -> AnyhowResult<()> {
    let registry = load_model_registry()?;
    
    if registry.models.is_empty() {
        println!("No models installed. Use 'embed-tool model download' to add models.");
        return Ok(());
    }
    
    println!("{:<20} {:<15} {:<12} {:<10} {}", 
             "NAME", "SOURCE", "DIMENSIONS", "SIZE", "DESCRIPTION");
    println!("{}", "-".repeat(80));
    
    for (name, info) in &registry.models {
        let dims = info.dimensions.map(|d| d.to_string()).unwrap_or_else(|| "unknown".to_string());
        let size = info.size_mb.map(|s| format!("{:.1}MB", s)).unwrap_or_else(|| "unknown".to_string());
        let desc = info.description.as_deref().unwrap_or("");
        
        println!("{:<20} {:<15} {:<12} {:<10} {}", 
                 name, info.source, dims, size, desc);
    }
    
    println!("\nBuilt-in models:");
    println!("  potion-8M      huggingface   8            ~32MB     Small, fast model");
    println!("  potion-32M     huggingface   32           ~128MB    Balanced model (default)");
    
    Ok(())
}

async fn download_model(args: DownloadArgs) -> AnyhowResult<()> {
    let model_name = args.alias.unwrap_or_else(|| args.model_name.clone());
    let models_dir = get_models_dir()?;
    let model_path = models_dir.join(&model_name);
    
    if model_path.exists() && !args.force {
        eprintln!("Model '{}' already exists. Use --force to overwrite.", model_name);
        return Ok(());
    }
    
    println!("Downloading model '{}' from '{}'...", model_name, args.model_name);
    
    // Create models directory if it doesn't exist
    fs::create_dir_all(&models_dir)?;
    
    // This would integrate with model2vec's download functionality
    // For now, we'll simulate the download
    println!("⚠️  Model download not yet implemented - would download from HuggingFace");
    println!("   Model: {}", args.model_name);
    println!("   Alias: {}", model_name);
    println!("   Path: {}", model_path.display());
    
    // Add to registry
    let mut registry = load_model_registry().unwrap_or_default();
    registry.models.insert(model_name.clone(), ModelInfo {
        name: model_name.clone(),
        path: model_path.to_string_lossy().to_string(),
        source: "huggingface".to_string(),
        dimensions: None, // Would be determined after download
        size_mb: None,
        downloaded_at: chrono::Utc::now().to_rfc3339(),
        description: Some(format!("Downloaded from {}", args.model_name)),
    });
    
    save_model_registry(&registry)?;
    println!("✓ Model '{}' added to registry", model_name);
    
    Ok(())
}

async fn distill_model(args: DistillArgs) -> AnyhowResult<()> {
    let models_dir = get_models_dir()?;
    let output_path = if args.output.starts_with('/') || args.output.contains(':') {
        PathBuf::from(&args.output)
    } else {
        models_dir.join(&args.output)
    };
    
    if output_path.exists() && !args.force {
        eprintln!("Output model '{}' already exists. Use --force to overwrite.", args.output);
        return Ok(());
    }
    
    println!("Distilling model...");
    println!("  Input: {}", args.input);
    println!("  Output: {}", output_path.display());
    println!("  Dimensions: {}", args.dims);
    
    // Create output directory if needed
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Call the distillation function from utils

    crate::utils::distill(&args.input, 128, Some(output_path.clone())).await.map_err(|e| anyhow::anyhow!("Distillation failed: {}", e))?;



    // Add to registry
    let mut registry = load_model_registry().unwrap_or_default();
    registry.models.insert(args.output.clone(), ModelInfo {
        name: args.output.clone(),
        path: output_path.to_string_lossy().to_string(),
        source: "distilled".to_string(),
        dimensions: Some(args.dims),
        size_mb: get_directory_size(&output_path),
        downloaded_at: chrono::Utc::now().to_rfc3339(),
        description: Some(format!("Distilled from {} with {} dimensions", args.input, args.dims)),
    });
    
    save_model_registry(&registry)?;
    println!("✓ Model '{}' distilled and added to registry", args.output);
    
    Ok(())
}

async fn remove_model(args: RemoveArgs) -> AnyhowResult<()> {
    let mut registry = load_model_registry()?;
    
    if let Some(model_info) = registry.models.get(&args.model_name) {
        if !args.yes {
            print!("Remove model '{}' at '{}'? [y/N]: ", args.model_name, model_info.path);
            use std::io::{self, Write};
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Cancelled.");
                return Ok(());
            }
        }
        
        // Remove the model files
        let model_path = PathBuf::from(&model_info.path);
        if model_path.exists() {
            if model_path.is_dir() {
                fs::remove_dir_all(&model_path)?;
            } else {
                fs::remove_file(&model_path)?;
            }
        }
        
        // Remove from registry
        registry.models.remove(&args.model_name);
        save_model_registry(&registry)?;
        
        println!("✓ Model '{}' removed", args.model_name);
    } else {
        eprintln!("Model '{}' not found in registry", args.model_name);
    }
    
    Ok(())
}

async fn update_model(args: UpdateArgs) -> AnyhowResult<()> {
    let registry = load_model_registry()?;
    
    if let Some(model_info) = registry.models.get(&args.model_name) {
        match model_info.source.as_str() {
            "huggingface" => {
                println!("Re-downloading model '{}' from HuggingFace...", args.model_name);
                // Would re-download the model
                println!("⚠️  Model update not yet implemented");
            }
            "distilled" => {
                println!("Cannot update distilled model '{}'. Create a new distillation instead.", args.model_name);
            }
            "local" => {
                println!("Cannot update local model '{}'. Manual update required.", args.model_name);
            }
            _ => {
                println!("Unknown model source for '{}'", args.model_name);
            }
        }
    } else {
        eprintln!("Model '{}' not found in registry", args.model_name);
    }
    
    Ok(())
}

async fn show_model_info(args: InfoArgs) -> AnyhowResult<()> {
    let registry = load_model_registry()?;
    
    if let Some(model_info) = registry.models.get(&args.model_name) {
        println!("Model Information:");
        println!("  Name: {}", model_info.name);
        println!("  Path: {}", model_info.path);
        println!("  Source: {}", model_info.source);
        
        if let Some(dims) = model_info.dimensions {
            println!("  Dimensions: {}", dims);
        }
        
        if let Some(size) = model_info.size_mb {
            println!("  Size: {:.1} MB", size);
        }
        
        println!("  Downloaded: {}", model_info.downloaded_at);
        
        if let Some(desc) = &model_info.description {
            println!("  Description: {}", desc);
        }
        
        // Check if files exist
        let model_path = PathBuf::from(&model_info.path);
        if model_path.exists() {
            println!("  Status: ✓ Available");
        } else {
            println!("  Status: ✗ Missing files");
        }
    } else {
        // Check built-in models
        match args.model_name.as_str() {
            "potion-8M" => {
                println!("Built-in Model: potion-8M");
                println!("  Source: minishlab/potion-base-8M (HuggingFace)");
                println!("  Dimensions: 8");
                println!("  Size: ~32 MB");
                println!("  Description: Small, fast embedding model");
            }
            "potion-32M" => {
                println!("Built-in Model: potion-32M");
                println!("  Source: minishlab/potion-base-32M (HuggingFace)");
                println!("  Dimensions: 32");
                println!("  Size: ~128 MB");
                println!("  Description: Balanced embedding model (default)");
            }
            _ => {
                eprintln!("Model '{}' not found", args.model_name);
            }
        }
    }
    
    Ok(())
}

fn get_models_dir() -> AnyhowResult<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    
    Ok(PathBuf::from(home).join(".embed-tool").join("models"))
}

fn get_registry_path() -> AnyhowResult<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    
    Ok(PathBuf::from(home).join(".embed-tool").join("models.json"))
}

fn load_model_registry() -> AnyhowResult<ModelRegistry> {
    let registry_path = get_registry_path()?;
    
    if !registry_path.exists() {
        return Ok(ModelRegistry {
            models: HashMap::new(),
        });
    }
    
    let content = fs::read_to_string(registry_path)?;
    let registry: ModelRegistry = serde_json::from_str(&content)?;
    Ok(registry)
}

fn save_model_registry(registry: &ModelRegistry) -> AnyhowResult<()> {
    let registry_path = get_registry_path()?;
    
    // Create directory if it doesn't exist
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let content = serde_json::to_string_pretty(registry)?;
    fs::write(registry_path, content)?;
    Ok(())
}

fn get_directory_size(path: &PathBuf) -> Option<f64> {
    if path.is_dir() {
        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    size += metadata.len();
                }
            }
        }
        Some(size as f64 / 1024.0 / 1024.0) // Convert to MB
    } else if let Ok(metadata) = fs::metadata(path) {
        Some(metadata.len() as f64 / 1024.0 / 1024.0)
    } else {
        None
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self {
            models: HashMap::new(),
        }
    }
}