use crate::cli::{ConfigAction, SetConfigArgs, EmbedArgs, BatchArgs};
use std::path::PathBuf;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Config {
    server: ServerConfig,
    auth: AuthConfig,
    models: ModelConfig,
    logging: LoggingConfig,
}

#[derive(Serialize, Deserialize)]
struct ServerConfig {
    default_port: u16,
    default_bind: String,
    default_model: String,
    enable_mcp: bool,
    rate_limit_rps: u32,
    rate_limit_burst: u32,
}

#[derive(Serialize, Deserialize)]
struct AuthConfig {
    require_api_key: bool,
    registration_enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct ModelConfig {
    models_dir: Option<String>,
    auto_download: bool,
    default_distill_dims: usize,
}

#[derive(Serialize, Deserialize)]
struct LoggingConfig {
    level: String,
    file: Option<String>,
    json_format: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            default_port: 8080,
            default_bind: "0.0.0.0".to_string(),
            default_model: "potion-32M".to_string(),
            enable_mcp: false,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            require_api_key: true,
            registration_enabled: true,
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            models_dir: None,
            auto_download: true,
            default_distill_dims: 128,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
            json_format: false,
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
    _config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚠️  Direct embedding not yet implemented - would embed:");
    println!("  Text: \"{}\"", args.text);
    let model_name = args.model.as_deref().unwrap_or("default");
    println!("  Model: {}", model_name);
    println!("  Format: {}", args.format);
    println!("\nStart the server first with: embed-tool server start");
    println!("Then use: curl -X POST http://localhost:8080/v1/embeddings \\");
    println!("  -H \"Content-Type: application/json\" \\");
    println!("  -d '{{\"input\": [\"{}\"], \"model\": \"{}\"}}'", 
             args.text, args.model.as_deref().unwrap_or("potion-32M"));
    
    Ok(())
}

pub async fn handle_batch_command(
    args: BatchArgs,
    _config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚠️  Batch embedding not yet implemented - would process:");
    println!("  Input: {}", args.input.display());
    
    if let Some(output) = &args.output {
        println!("  Output: {}", output.display());
    }
    
    let model_name = args.model.as_deref().unwrap_or("default");
    println!("  Model: {}", model_name);
    println!("  Format: {}", args.format);
    println!("  Batch size: {}", args.batch_size);
    
    // Check if input file exists
    if !args.input.exists() {
        eprintln!("Error: Input file '{}' does not exist", args.input.display());
        return Ok(());
    }
    
    println!("\nStart the server first with: embed-tool server start");
    println!("Then implement batch processing via the HTTP API");
    
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
    println!("enable_mcp = {}", config.server.enable_mcp);
    println!("rate_limit_rps = {}", config.server.rate_limit_rps);
    println!("rate_limit_burst = {}", config.server.rate_limit_burst);
    
    println!("\n[auth]");
    println!("require_api_key = {}", config.auth.require_api_key);
    println!("registration_enabled = {}", config.auth.registration_enabled);
    
    println!("\n[models]");
    if let Some(models_dir) = &config.models.models_dir {
        println!("models_dir = \"{}\"", models_dir);
    }
    println!("auto_download = {}", config.models.auto_download);
    println!("default_distill_dims = {}", config.models.default_distill_dims);
    
    println!("\n[logging]");
    println!("level = \"{}\"", config.logging.level);
    if let Some(file) = &config.logging.file {
        println!("file = \"{}\"", file);
    }
    println!("json_format = {}", config.logging.json_format);
    
    Ok(())
}

async fn set_config(
    args: SetConfigArgs,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = load_config(config_path.clone()).unwrap_or_default();
    
    // Parse the key path (e.g., "server.default_port" or "auth.require_api_key")
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
        ["server", "enable_mcp"] => {
            config.server.enable_mcp = value.parse()?;
        }
        ["server", "rate_limit_rps"] => {
            config.server.rate_limit_rps = value.parse()?;
        }
        ["server", "rate_limit_burst"] => {
            config.server.rate_limit_burst = value.parse()?;
        }
        ["auth", "require_api_key"] => {
            config.auth.require_api_key = value.parse()?;
        }
        ["auth", "registration_enabled"] => {
            config.auth.registration_enabled = value.parse()?;
        }
        ["models", "models_dir"] => {
            config.models.models_dir = Some(value);
        }
        ["models", "auto_download"] => {
            config.models.auto_download = value.parse()?;
        }
        ["models", "default_distill_dims"] => {
            config.models.default_distill_dims = value.parse()?;
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
            eprintln!("  server.enable_mcp, server.rate_limit_rps, server.rate_limit_burst");
            eprintln!("  auth.require_api_key, auth.registration_enabled");
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
    
    Ok(PathBuf::from(home).join(".embed-tool").join("config.toml"))
}

fn load_config(config_path: Option<PathBuf>) -> Result<Config, Box<dyn std::error::Error>> {
    let config_file_path = get_config_path(config_path)?;
    
    if !config_file_path.exists() {
        return Ok(Config::default());
    }
    
    let content = fs::read_to_string(config_file_path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

fn save_config(config: &Config, config_path: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
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
    use std::env;
    use std::fs;
    use std::sync::Mutex;

    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    fn with_test_env<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _lock = TEST_MUTEX.lock().unwrap();
        // Save original HOME
        let original_home = env::var("HOME").ok();
        let original_userprofile = env::var("USERPROFILE").ok();

        // Create a temporary directory for testing
        let temp_dir = std::env::temp_dir().join("embed_tool_config_test").join(format!("test_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        // Set temporary HOME
        unsafe { env::set_var("HOME", &temp_dir) };

        let result = f();

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
        // Restore original environment
        if let Some(home) = original_home {
            unsafe { env::set_var("HOME", home) };
        } else {
            unsafe { env::remove_var("HOME") };
        }
        if let Some(userprofile) = original_userprofile {
            unsafe { env::set_var("USERPROFILE", userprofile) };
        } else {
            unsafe { env::remove_var("USERPROFILE") };
        }

        result
    }

    #[test]
    fn test_get_config_path_default() {
        with_test_env(|| {
            let result = get_config_path(None).unwrap();
            assert!(result.ends_with(".embed-tool/config.toml"));
        });
    }

    #[test]
    fn test_get_config_path_custom() {
        let custom_path = PathBuf::from("/custom/path/config.toml");
        let result = get_config_path(Some(custom_path.clone())).unwrap();
        assert_eq!(result, custom_path);
    }

    #[test]
    fn test_load_config_defaults() {
        with_test_env(|| {
            let config_path = get_config_path(None).unwrap();
            // Ensure config file doesn't exist
            assert!(!config_path.exists());

            let config = load_config(None).unwrap();
            // Check default values
            assert_eq!(config.server.default_port, 8080);
            assert_eq!(config.server.default_bind, "0.0.0.0");
            assert_eq!(config.server.default_model, "potion-32M");
            assert!(!config.server.enable_mcp);
            assert_eq!(config.auth.require_api_key, true);
            assert_eq!(config.auth.registration_enabled, true);
            assert_eq!(config.logging.level, "info");
        });
    }

    #[test]
    fn test_save_and_load_config() {
        with_test_env(|| {
            let config_path = get_config_path(None).unwrap();

            let mut config = Config::default();
            config.server.default_port = 9090;
            config.server.default_model = "custom-model".to_string();
            config.auth.require_api_key = false;

            save_config(&config, None).unwrap();
            assert!(config_path.exists());

            let loaded = load_config(None).unwrap();
            assert_eq!(loaded.server.default_port, 9090);
            assert_eq!(loaded.server.default_model, "custom-model");
            assert_eq!(loaded.auth.require_api_key, false);
        });
    }

    #[test]
    fn test_show_config() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Should not panic
                let result = show_config(None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_show_config_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Should not panic
                let result = show_config_path(None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_set_config_server_values() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test setting server.default_port
                let args = SetConfigArgs {
                    key: "server.default_port".to_string(),
                    value: "9090".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                // Verify the change
                let config = load_config(None).unwrap();
                assert_eq!(config.server.default_port, 9090);
            });
        });
    }

    #[test]
    fn test_set_config_auth_values() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test setting auth.require_api_key
                let args = SetConfigArgs {
                    key: "auth.require_api_key".to_string(),
                    value: "false".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                // Verify the change
                let config = load_config(None).unwrap();
                assert_eq!(config.auth.require_api_key, false);
            });
        });
    }

    #[test]
    fn test_set_config_logging_level() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test setting logging.level
                let args = SetConfigArgs {
                    key: "logging.level".to_string(),
                    value: "debug".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                // Verify the change
                let config = load_config(None).unwrap();
                assert_eq!(config.logging.level, "debug");
            });
        });
    }

    #[test]
    fn test_set_config_invalid_key() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "invalid.key".to_string(),
                    value: "value".to_string(),
                };
                // Should not panic, just print error
                let result = set_config(args, None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_set_config_invalid_log_level() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "logging.level".to_string(),
                    value: "invalid".to_string(),
                };
                // Should not panic, just print error
                let result = set_config(args, None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_handle_embed_command() {
        let args = EmbedArgs {
            text: "Hello world".to_string(),
            model: Some("test-model".to_string()),
            format: "json".to_string(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Should not panic
            let result = handle_embed_command(args, None).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_batch_command() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let temp_dir = std::env::temp_dir().join("embed_tool_batch_test");
                fs::create_dir_all(&temp_dir).unwrap();
                let input_file = temp_dir.join("input.json");
                fs::write(&input_file, "[]").unwrap();

                let args = BatchArgs {
                    input: input_file,
                    output: Some(temp_dir.join("output.json")),
                    model: Some("test-model".to_string()),
                    format: "json".to_string(),
                    batch_size: 32,
                };

                // Should not panic
                let result = handle_batch_command(args, None).await;
                assert!(result.is_ok());

                // Cleanup
                let _ = fs::remove_dir_all(&temp_dir);
            });
        });
    }

    #[test]
    fn test_handle_batch_command_missing_input() {
        let args = BatchArgs {
            input: PathBuf::from("/nonexistent/file.json"),
            output: None,
            model: None,
            format: "json".to_string(),
            batch_size: 32,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Should not panic, just print error
            let result = handle_batch_command(args, None).await;
            assert!(result.is_ok());
        });
    }
}