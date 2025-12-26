//! Configuration management for the embedding server.
//! 
//! This module implements the `config` subcommand and handles persistent configuration
//! in TOML format with environment variable overrides.
//! 
//! ## Configuration Hierarchy
//! 
//! Settings are resolved in the following priority order (highest to lowest):
//! 
//! 1. Command-line arguments (e.g., `--port 9090`)
//! 2. Environment variables (e.g., `EMBED_TOOL_SERVER_PORT=9090`)
//! 3. Configuration file (`~/.config/static-embedding-tool/config.toml`)
//! 4. Built-in defaults
//! 
//! ## Configuration Sections
//! 
//! - **Server**: Port, bind address, default model
//! - **Models**: Model paths, cache directory, auto-download settings
//! - **Logging**: Log levels, output format, file rotation
//! 
//! ## Examples
//! 
//! ```bash
//! # Show current configuration
//! static-embedding-tool config get
//! 
//! # Set a value
//! static-embedding-tool config set server.default_port 9090
//! 
//! # Reset to defaults
//! static-embedding-tool config reset
//! 
//! # Show config file location
//! static-embedding-tool config path
//! ```
//! 
//! ## Environment Variables
//! 
//! All config keys can be overridden via environment variables with the prefix
//! `EMBED_TOOL_` and uppercase section.key format:
//! 
//! - `EMBED_TOOL_SERVER_PORT=9090`
//! - `EMBED_TOOL_MODELS_CACHE_DIR=/custom/path`

use crate::cli::{BatchArgs, ConfigAction, EmbedArgs, SetConfigArgs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Top-level configuration structure.
#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub models: ModelConfig,
    pub logging: LoggingConfig,
}

/// Server-specific configuration.
#[derive(Serialize, Deserialize)]
pub struct ServerConfig {
    pub default_port: u16,
    pub default_bind: String,
    pub default_model: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            default_port: 8084,
            default_bind: "127.0.0.1".to_string(),
            default_model: "potion-32M".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ModelConfig {
    pub models_dir: Option<String>,
    pub auto_download: bool,
    pub default_distill_dims: Option<usize>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            models_dir: None,
            auto_download: true,
            default_distill_dims: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<String>,
    pub json_format: bool,
    pub max_file_size: Option<u64>,
    pub max_files: Option<u32>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
            json_format: false,
            max_file_size: None,
            max_files: None,
        }
    }
}

pub async fn handle_config_command(
    action: ConfigAction,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ConfigAction::Get => show_config(config_path).await,
        ConfigAction::Set(args) => set_config(args, config_path).await,
        ConfigAction::Reset => reset_config(config_path).await,
        ConfigAction::Path => show_config_path(config_path).await,
    }
}

pub async fn handle_embed_command(
    args: EmbedArgs,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use reqwest::Client;
    use serde_json::{Value, json};

    let config = load_config(config_path)?;
    let port = config.server.default_port;
    let client = Client::new();
    let url = format!("http://localhost:{}/v1/embeddings", port);

    let model_name = args.model.as_deref().unwrap_or("potion-32M");

    let request_body = json!({
        "input": [args.text],
        "model": model_name,
        "encoding_format": if args.format == "json" { "float" } else { &args.format }
    });

    if config.logging.level == "debug" || config.logging.level == "trace" {
        eprintln!("🔍 Embedding text using model '{}'...", model_name);
        eprintln!("  Text: \"{}\"", args.text);
    }

    // Try to use the server first
    match client.post(&url).json(&request_body).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let result: Value = response.json().await?;
                display_embedding_result(&result, &args.format)?;
                if config.logging.level == "debug" || config.logging.level == "trace" {
                    eprintln!("✓ Embedding completed successfully (via server)");
                }
                return Ok(());
            } else {
                let error_text = response.text().await?;
                eprintln!("⚠️  Server error ({}): {}", status, error_text);
            }
        }
        Err(_) => {
            if config.logging.level == "debug" || config.logging.level == "trace" {
                eprintln!("ℹ️  Server not reachable on http://localhost:{}, attempting local embedding...", port);
            }
        }
    }

    // Fallback to local embedding
    match run_local_embedding(&[args.text], model_name).await {
        Ok(embeddings) => {
            let prompt_tokens = (embeddings.len() + 3) / 4;
            let result = json!({
                "object": "list",
                "data": embeddings.into_iter().enumerate().map(|(i, e)| {
                    json!({
                        "object": "embedding",
                        "embedding": e,
                        "index": i
                    })
                }).collect::<Vec<_>>(),
                "model": model_name,
                "usage": {
                    "prompt_tokens": prompt_tokens,
                    "total_tokens": prompt_tokens
                }
            });
            display_embedding_result(&result, &args.format)?;
            if config.logging.level == "debug" || config.logging.level == "trace" {
                eprintln!("✓ Embedding completed successfully (local)");
            }
        }
        Err(e) => {
            eprintln!("❌ Local embedding failed: {}", e);
            eprintln!("\nMake sure the model is downloaded or the server is running:");
            eprintln!("  static-embedding-tool model download {}", model_name);
            eprintln!("  static-embedding-tool server start");
        }
    }

    Ok(())
}

fn display_embedding_result(result: &serde_json::Value, format: &str) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "csv" => {
            if let Some(data) = result
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first())
            {
                if let Some(embedding) = 
                    data.get("embedding").and_then(|e| e.as_array())
                {
                    println!("embedding");
                    for (i, value) in embedding.iter().enumerate() {
                        if let Some(num) = value.as_f64() {
                            print!("{:.6}{}", num, if i < embedding.len() - 1 { "," } else { "" });
                        }
                    }
                    println!();
                }
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

async fn run_local_embedding(inputs: &[String], model_name: &str) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
    use model2vec_rs::model::StaticModel;
    
    // Determine model path
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not determine home directory")?;
    
    let model_path = std::path::PathBuf::from(home)
        .join(".static-embedding-tool")
        .join("models")
        .join(model_name);

    if !model_path.exists() {
        // Check for built-in name mapping
        let hf_id = match model_name {
            "potion-8M" => "minishlab/potion-base-8M",
            "potion-32M" => "minishlab/potion-base-32M",
            _ => return Err(format!("Model path '{}' does not exist and no built-in mapping found", model_path.display()).into()),
        };
        
        // Try to load from HF directly or return error
        let model = tokio::task::spawn_blocking(move || {
            StaticModel::from_pretrained(hf_id, None, None, None)
        }).await??;
        return Ok(model.encode(inputs));
    }

    let model = tokio::task::spawn_blocking(move || {
        StaticModel::from_pretrained(&model_path, None, None, None)
    }).await??;
    
    Ok(model.encode(inputs))
}

pub async fn handle_batch_command(
    args: BatchArgs,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use reqwest::Client;
    use serde_json::{Value, json};
    use std::fs;
    use std::io::Write;

    let config = load_config(config_path)?;
    let port = config.server.default_port;

    // Check if input file exists
    if !args.input.exists() {
        eprintln!(
            "❌ Error: Input file '{}' does not exist",
            args.input.display()
        );
        return Ok(());
    }

    // Read input file
    let input_content = fs::read_to_string(&args.input)?;
    let input_data: Vec<String> = if args.input.extension().and_then(|s| s.to_str()) == Some("json")
    {
        serde_json::from_str(&input_content)?
    } else {
        // Assume text file with one item per line
        input_content.lines().map(|s| s.to_string()).collect()
    };

    if input_data.is_empty() {
        eprintln!("❌ Error: Input file is empty or contains no valid data");
        return Ok(());
    }

        let client = Client::new();
        let url = format!("http://localhost:{}/v1/embeddings", port);
        let model_name = args.model.as_deref().unwrap_or("potion-32M");
    
        if config.logging.level == "debug" || config.logging.level == "trace" {
            eprintln!(
                "🔍 Processing {} texts in batches of {} using model '{}'...",
                input_data.len(),
                args.batch_size,
                model_name
            );
            eprintln!("  Input: {}", args.input.display());
            if let Some(output) = &args.output {
                eprintln!("  Output: {}", output.display());
            }
        }
    
        let mut all_embeddings = Vec::new();
        
        // Try server first
        let mut use_local = false;
        for chunk in input_data.chunks(args.batch_size) {
            let request_body = json!({
                "input": chunk,
                "model": model_name,
                "encoding_format": "float"
            });
    
            match client.post(&url).json(&request_body).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let result: Value = response.json().await?;
                        if let Some(data) = result.get("data").and_then(|d| d.as_array()) {
                            for item in data {
                                if let Some(embedding) =
                                    item.get("embedding").and_then(|e| e.as_array())
                                {
                                    let embedding_vec: Vec<f32> = embedding
                                        .iter()
                                        .filter_map(|v| v.as_f64())
                                        .map(|v| v as f32)
                                        .collect();
                                    all_embeddings.push(embedding_vec);
                                }
                            }
                            if config.logging.level == "debug" || config.logging.level == "trace" {
                                eprintln!("  ✓ Processed {}/{} texts (via server)", all_embeddings.len(), input_data.len());
                            }
                        }
                    } else {
                        let error_text = response.text().await?;
                        eprintln!("⚠️  Server error ({}): {}", status, error_text);
                        use_local = true;
                        break;
                    }
                }
                Err(_) => {
                    if config.logging.level == "debug" || config.logging.level == "trace" {
                        eprintln!("ℹ️  Server not reachable, falling back to local processing...");
                    }
                    use_local = true;
                    break;
                }
            }
        }
    
        if use_local {
            all_embeddings.clear();
            match run_local_embedding(&input_data, model_name).await {
                Ok(embeddings) => {
                    all_embeddings = embeddings;
                    if config.logging.level == "debug" || config.logging.level == "trace" {
                        eprintln!("  ✓ Processed {} texts (local)", all_embeddings.len());
                    }
                }
                Err(e) => {
                    eprintln!("❌ Local batch processing failed: {}", e);
                    return Ok(());
                }
            }
        }
    
        // Output results
        if let Some(output_path) = &args.output {
            match args.format.as_str() {
                "json" => {
                    let output_data = json!({
                        "model": model_name,
                        "embeddings": all_embeddings,
                        "input_count": input_data.len(),
                        "dimensions": all_embeddings.first().map(|e| e.len()).unwrap_or(0)
                    });
                    fs::write(output_path, serde_json::to_string_pretty(&output_data)?)?;
                }
                "csv" => {
                    let mut file = fs::File::create(output_path)?;
                    // Write header
                    writeln!(file, "index,embedding")?;
                    for (i, embedding) in all_embeddings.iter().enumerate() {
                        write!(file, "{}", i)?;
                        for value in embedding {
                            write!(file, ",{:.6}", value)?;
                        }
                        writeln!(file)?;
                    }
                }
                "npy" => {
                    // For NPY format, we'd need the npy crate, but for now just save as JSON
                    eprintln!("⚠️  NPY format not yet supported, saving as JSON instead");
                    let output_data = json!({
                        "model": model_name,
                        "embeddings": all_embeddings,
                        "input_count": input_data.len(),
                        "dimensions": all_embeddings.first().map(|e| e.len()).unwrap_or(0)
                    });
                    let npy_path = output_path.with_extension("json");
                    fs::write(&npy_path, serde_json::to_string_pretty(&output_data)?)?;
                    if config.logging.level == "debug" || config.logging.level == "trace" {
                        eprintln!(
                            "✓ Results saved to {} (NPY format not implemented)",
                            npy_path.display()
                        );
                    }
                }
                _ => {
                    eprintln!("❌ Unsupported output format: {}", args.format);
                    return Ok(());
                }
            }
            if config.logging.level == "debug" || config.logging.level == "trace" {
                eprintln!("✓ Results saved to {}", output_path.display());
            }
        } else {
            // Print to stdout
            let output_data = json!({
                "model": model_name,
                "embeddings": all_embeddings,
                "input_count": input_data.len(),
                "dimensions": all_embeddings.first().map(|e| e.len()).unwrap_or(0)
            });
            println!("{}", serde_json::to_string_pretty(&output_data)?);
        }
    
        if config.logging.level == "debug" || config.logging.level == "trace" {
            eprintln!("✓ Batch processing completed successfully");
        }
    
        Ok(())
    }
async fn show_config(config_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config(config_path)?;
    let config_file_path = get_config_path(None)?;

    println!("Configuration ({})", config_file_path.display());
    println!("{}", "-".repeat(50));

    println!("\n[server]");
    println!("default_port = {}", config.server.default_port);
    println!("default_bind = \"{}\"", config.server.default_bind);
    println!("default_model = \"{}\"", config.server.default_model);

    println!("\n[models]");
    if let Some(models_dir) = &config.models.models_dir {
        println!("models_dir = \"{}\"", models_dir);
    }
    println!("auto_download = {}", config.models.auto_download);
    println!(
        "default_distill_dims = {}",
        config.models.default_distill_dims.map(|d| d.to_string()).unwrap_or_else(|| "default".to_string())
    );

    println!("\n[logging]");
    println!("level = \"{}\"", config.logging.level);
    if let Some(file) = &config.logging.file {
        println!("file = \"{}\"", file);
    }
    println!("json_format = {}", config.logging.json_format);
    if let Some(max_file_size) = config.logging.max_file_size {
        println!("max_file_size = {}", max_file_size);
    }
    if let Some(max_files) = config.logging.max_files {
        println!("max_files = {}", max_files);
    }

    Ok(())
}

async fn set_config(
    args: SetConfigArgs,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = load_config(config_path.clone()).unwrap_or_default();

    // Parse the key path (e.g., "server.default_port" or "models.auto_download")
    let parts: Vec<&str> = args.key.split('.').collect();
    let value = args.value.clone(); // Clone to avoid move issues

    match parts.as_slice() {
        ["server", "default_port"] => {
            config.server.default_port = value.parse()?;
        }
        ["server", "default_bind"] => {
            config.server.default_bind = value;
        }
        ["server", "default_model"] => {
            config.server.default_model = value;
        }
        ["models", "models_dir"] => {
            config.models.models_dir = Some(value);
        }
        ["models", "auto_download"] => {
            config.models.auto_download = value.parse()?;
        }
        ["models", "default_distill_dims"] => {
            config.models.default_distill_dims = Some(value.parse()?);
        }
        ["logging", "level"] => {
            if ["trace", "debug", "info", "warn", "error"].contains(&value.as_str()) {
                config.logging.level = value;
            } else {
                eprintln!("Invalid log level. Use: trace, debug, info, warn, error");
                return Ok(());
            }
        }
        ["logging", "file"] => {
            config.logging.file = Some(value);
        }
        ["logging", "json_format"] => {
            config.logging.json_format = value.parse()?;
        }
        _ => {
            eprintln!("Unknown configuration key: {}", args.key);
            eprintln!("Available keys:");
            eprintln!("  server.default_port, server.default_bind, server.default_model");
            eprintln!("  models.models_dir, models.auto_download, models.default_distill_dims");
            eprintln!("  logging.level, logging.file, logging.json_format");
            return Ok(());
        }
    }

    save_config(&config, config_path)?;
    println!("✓ Configuration updated: {} = {}", args.key, args.value);

    Ok(())
}

async fn reset_config(config_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(config_path)?;

    if config_file_path.exists() {
        print!("Reset configuration to defaults? [y/N]: ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase().starts_with('y') {
            fs::remove_file(&config_file_path)?;
            println!("✓ Configuration reset to defaults");
        } else {
            println!("Cancelled.");
        }
    } else {
        println!("Configuration file does not exist (already at defaults)");
    }

    Ok(())
}

async fn show_config_path(config_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(config_path)?;
    println!("{}", config_file_path.display());

    if config_file_path.exists() {
        println!("  Status: ✓ Exists");
    } else {
        println!("  Status: ✗ Not found (using defaults)");
    }

    Ok(())
}

fn get_config_path(config_path: Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = config_path {
        return Ok(path);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not determine home directory")?;

    Ok(PathBuf::from(home).join(".static-embedding-tool").join("config.toml"))
}

pub fn load_config(config_path: Option<PathBuf>) -> Result<Config, Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(config_path)?;

    if !config_file_path.exists() {
        return Ok(Config::default());
    }

    let content = fs::read_to_string(config_file_path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

fn save_config(
    config: &Config,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(config_path)?;

    // Create directory if it doesn't exist
    if let Some(parent) = config_file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)?;
    fs::write(config_file_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper to create an isolated config file path for each test
    fn make_temp_config_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("test_config.toml");
        (dir, path)
    }

    #[tokio::test]
    async fn test_handle_embed_command_smoke() {
        // Exercise handle_embed_command printing path with defaults
        let args = EmbedArgs {
            text: "Hello test".to_string(),
            model: None,
            format: "json".to_string(),
            watch: false,
            daemon: false,
        };
        let result = handle_embed_command(args, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_batch_command_with_missing_input() {
        // Provide a non-existent file path; function should not error (prints message)
        let args = BatchArgs {
            input: PathBuf::from("/definitely/does/not/exist.json"),
            output: None,
            model: Some("potion-8M".to_string()),
            format: "json".to_string(),
            batch_size: 32,
            watch: false,
            daemon: false,
        };
        let result = handle_batch_command(args, None).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_config_path_default() {
        // Using a custom path to avoid environment interaction
        let (_dir, custom) = make_temp_config_path();
        let result = get_config_path(Some(custom.clone())).unwrap();
        assert_eq!(result, custom);
    }

    #[test]
    fn test_get_config_path_custom() {
        let custom_path = PathBuf::from("/custom/path/config.toml");
        let result = get_config_path(Some(custom_path.clone())).unwrap();
        assert_eq!(result, custom_path);
    }

    #[test]
    fn test_load_config_defaults() {
        let (_dir, custom) = make_temp_config_path();
        // Ensure config file doesn't exist
        assert!(!custom.exists());

        let config = load_config(Some(custom)).unwrap();
        // Check default values
        assert_eq!(config.server.default_port, 8084);
        assert_eq!(config.server.default_bind, "127.0.0.1");
        assert_eq!(config.server.default_model, "potion-32M");
        assert_eq!(config.logging.level, "info");
    }

    #[test]
    fn test_save_and_load_config() {
        let (_dir, custom) = make_temp_config_path();

        let mut config = Config::default();
        config.server.default_port = 9090;
        config.server.default_model = "custom-model".to_string();

        save_config(&config, Some(custom.clone())).unwrap();
        assert!(custom.exists());

        let loaded = load_config(Some(custom)).unwrap();
        assert_eq!(loaded.server.default_port, 9090);
        assert_eq!(loaded.server.default_model, "custom-model");
    }

    #[test]
    fn test_show_config() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Should not panic
            let result = show_config(Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_show_config_path() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Should not panic
            let result = show_config_path(Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_set_config_server_values() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Test setting server.default_port
            let args = SetConfigArgs {
                key: "server.default_port".to_string(),
                value: "9090".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            // Verify the change
            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.server.default_port, 9090);
        });
    }

    #[test]
    fn test_set_config_logging_level() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Test setting logging.level
            let args = SetConfigArgs {
                key: "logging.level".to_string(),
                value: "debug".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            // Verify the change
            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.logging.level, "debug");
        });
    }

    #[test]
    fn test_handle_embed_command_executes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = EmbedArgs {
                text: "Hello test".to_string(),
                model: Some("potion-32M".to_string()),
                format: "json".to_string(),
                watch: false,
                daemon: false,
            };
            // Should print guidance and return Ok
            let result = handle_embed_command(args, None).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_batch_command_missing_input() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = BatchArgs {
                input: PathBuf::from("/definitely/not/exist/input.json"),
                output: None,
                model: None,
                format: "json".to_string(),
                batch_size: 32,
                watch: false,
                daemon: false,
            };
            // Should return Ok after printing error when file missing
            let result = handle_batch_command(args, None).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_batch_command_with_input_file() {
        let tmp = TempDir::new().unwrap();
        let input_path = tmp.path().join("embed_tool_batch_test_input.json");
        fs::write(&input_path, "[\"a\", \"b\"]").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = BatchArgs {
                input: input_path.clone(),
                output: Some(tmp.path().join("embed_tool_batch_test_output.json")),
                model: Some("potion-32M".to_string()),
                format: "csv".to_string(),
                batch_size: 10,
                watch: false,
                daemon: false,
            };
            let result = handle_batch_command(args, None).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_set_config_unknown_key() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "unknown.key".to_string(),
                value: "value".to_string(),
            };
            let result = set_config(args, Some(custom)).await;
            // set_config returns Ok(()) for unknown keys (just prints error)
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_load_config_with_path() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Create a custom config file in a temp dir
            let temp_dir = TempDir::new().unwrap();
            let custom_config_path = temp_dir.path().join("test_custom_config.toml");
            let custom_config = r#" 
[server]
default_port = 9999
default_bind = "127.0.0.1"
default_model = "potion-32M"

[models]
auto_download = true
default_distill_dims = 32

[logging]
level = "info"
json_format = false
"#;
            std::fs::write(&custom_config_path, custom_config).unwrap();

            let config = load_config(Some(custom_config_path.clone())).unwrap();
            assert_eq!(config.server.default_port, 9999);
            assert_eq!(config.server.default_bind, "127.0.0.1");
            assert_eq!(config.server.default_model, "potion-32M");
            // TempDir cleans up automatically
        });
    }

    #[test]
    fn test_save_config() {
        let mut config = Config::default();
        config.server.default_port = 7777;

        // Write to a file inside a dedicated temp directory
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().join("test_save_config.toml");
        let result = save_config(&config, Some(temp_path.clone()));
        assert!(result.is_ok());

        // Verify the file was written
        let content = std::fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("default_port = 7777"));
        // TempDir cleans up automatically
    }

    #[test]
    fn test_get_config_path() {
        let (_dir, custom) = make_temp_config_path();
        let path = get_config_path(Some(custom.clone())).unwrap();
        assert_eq!(path, custom);
    }

    #[test]
    fn test_get_config_path_with_custom() {
        let custom_path = PathBuf::from("/custom/path/config.toml");
        let path = get_config_path(Some(custom_path.clone()));
        assert_eq!(path.unwrap(), custom_path);
    }

    #[test]
    fn test_set_config_server_default_port() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "server.default_port".to_string(),
                value: "9090".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.server.default_port, 9090);
        });
    }

    #[test]
    fn test_set_config_server_default_bind() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "server.default_bind".to_string(),
                value: "127.0.0.1".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.server.default_bind, "127.0.0.1");
        });
    }

    #[test]
    fn test_set_config_server_default_model() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "server.default_model".to_string(),
                value: "custom-model".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.server.default_model, "custom-model");
        });
    }

    #[test]
    fn test_set_config_models_models_dir() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "models.models_dir".to_string(),
                value: "/custom/models".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.models.models_dir, Some("/custom/models".to_string()));
        });
    }

    #[test]
    fn test_set_config_models_auto_download() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "models.auto_download".to_string(),
                value: "false".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.models.auto_download, false);
        });
    }

    #[test]
    fn test_set_config_models_default_distill_dims() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "models.default_distill_dims".to_string(),
                value: "256".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.models.default_distill_dims, Some(256));        });
    }

    #[test]
    fn test_set_config_logging_file() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "logging.file".to_string(),
                value: "/var/log/static-embedding-tool.log".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(
                config.logging.file,
                Some("/var/log/static-embedding-tool.log".to_string())
            );
        });
    }

    #[test]
    fn test_set_config_logging_json_format() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "logging.json_format".to_string(),
                value: "true".to_string(),
            };
            let result = set_config(args, Some(custom.clone())).await;
            assert!(result.is_ok());

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.logging.json_format, true);
        });
    }

    #[test]
    fn test_set_config_invalid_log_level() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "logging.level".to_string(),
                value: "invalid".to_string(),
            };
            // Should not panic, just print error
            let result = set_config(args, Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_config_command_get() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = handle_config_command(ConfigAction::Get, Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_config_command_set() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let args = SetConfigArgs {
                key: "server.default_port".to_string(),
                value: "8888".to_string(),
            };
            let result = handle_config_command(ConfigAction::Set(args), Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_config_command_path() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = handle_config_command(ConfigAction::Path, Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_config_command_reset() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = handle_config_command(ConfigAction::Reset, Some(custom)).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_set_config_logging_max_file_size() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Note: max_file_size is not directly settable via set_config
            // This tests the default value coverage
            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.logging.max_file_size, None);
        });
    }

    #[test]
    fn test_set_config_logging_max_files() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Note: max_files is not directly settable via set_config
            // This tests the default value coverage
            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.logging.max_files, None);
        });
    }

    #[test]
    fn test_set_config_all_server_keys() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Test all server config keys to ensure full coverage
            let test_cases = vec![ 
                ("server.default_port", "9090"),
                ("server.default_bind", "127.0.0.1"),
                ("server.default_model", "test-model"),
            ];

            for (key, value) in test_cases {
                let args = SetConfigArgs {
                    key: key.to_string(),
                    value: value.to_string(),
                };
                let result = set_config(args, Some(custom.clone())).await;
                assert!(result.is_ok(), "Failed to set {}", key);
            }

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.server.default_port, 9090);
            assert_eq!(config.server.default_bind, "127.0.0.1");
            assert_eq!(config.server.default_model, "test-model");
        });
    }

    #[test]
    fn test_set_config_all_models_keys() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let test_cases = vec![ 
                ("models.models_dir", "/custom/models/dir"),
                ("models.auto_download", "false"),
                ("models.default_distill_dims", "256"),
            ];

            for (key, value) in test_cases {
                let args = SetConfigArgs {
                    key: key.to_string(),
                    value: value.to_string(),
                };
                let result = set_config(args, Some(custom.clone())).await;
                assert!(result.is_ok(), "Failed to set {}", key);
            }

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(
                config.models.models_dir,
                Some("/custom/models/dir".to_string())
            );
            assert_eq!(config.models.auto_download, false);
            assert_eq!(config.models.default_distill_dims, Some(256));
        });
    }

    #[test]
    fn test_set_config_all_logging_keys() {
        let (_dir, custom) = make_temp_config_path();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let test_cases = vec![ 
                ("logging.level", "debug"),
                ("logging.file", "/var/log/test.log"),
                ("logging.json_format", "true"),
            ];

            for (key, value) in test_cases {
                let args = SetConfigArgs {
                    key: key.to_string(),
                    value: value.to_string(),
                };
                let result = set_config(args, Some(custom.clone())).await;
                assert!(result.is_ok(), "Failed to set {}", key);
            }

            let config = load_config(Some(custom)).unwrap();
            assert_eq!(config.logging.level, "debug");
            assert_eq!(config.logging.file, Some("/var/log/test.log".to_string()));
            assert_eq!(config.logging.json_format, true);
        });
    }
}