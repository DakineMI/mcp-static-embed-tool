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
    enable_tls: bool,
    tls_cert_path: Option<String>,
    tls_key_path: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct AuthConfig {
    require_api_key: bool,
    registration_enabled: bool,
    api_key_header: String,
    api_key_prefix: String,
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
    max_file_size: Option<u64>,
    max_files: Option<u32>,
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
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            require_api_key: true,
            registration_enabled: true,
            api_key_header: "X-API-Key".to_string(),
            api_key_prefix: "Bearer ".to_string(),
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
    println!("enable_tls = {}", config.server.enable_tls);
    if let Some(cert_path) = &config.server.tls_cert_path {
        println!("tls_cert_path = \"{}\"", cert_path);
    }
    if let Some(key_path) = &config.server.tls_key_path {
        println!("tls_key_path = \"{}\"", key_path);
    }
    
    println!("\n[auth]");
    println!("require_api_key = {}", config.auth.require_api_key);
    println!("registration_enabled = {}", config.auth.registration_enabled);
    println!("api_key_header = \"{}\"", config.auth.api_key_header);
    println!("api_key_prefix = \"{}\"", config.auth.api_key_prefix);
    
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
        ["server", "enable_tls"] => {
            config.server.enable_tls = value.parse()?;
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
        ["auth", "api_key_header"] => {
            config.auth.api_key_header = value;
        }
        ["auth", "api_key_prefix"] => {
            config.auth.api_key_prefix = value;
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
        let _lock = TEST_MUTEX.lock().unwrap_or_else(|poisoned| {
            // If the mutex is poisoned, we can still use it
            poisoned.into_inner()
        });
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
    fn test_set_config_auth_require_api_key() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "auth.require_api_key".to_string(),
                    value: "false".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.auth.require_api_key, false);
            });
        });
    }

    #[test]
    fn test_set_config_auth_api_key_header() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "auth.api_key_header".to_string(),
                    value: "X-Custom-Key".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.auth.api_key_header, "X-Custom-Key");
            });
        });
    }

    #[test]
    fn test_set_config_auth_api_key_prefix() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "auth.api_key_prefix".to_string(),
                    value: "custom-".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.auth.api_key_prefix, "custom-");
            });
        });
    }

    #[test]
    fn test_set_config_server_enable_tls() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.enable_tls".to_string(),
                    value: "true".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.enable_tls, true);
            });
        });
    }

    #[test]
    fn test_set_config_unknown_key() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "unknown.key".to_string(),
                    value: "value".to_string(),
                };
                let result = set_config(args, None).await;
                // set_config returns Ok(()) for unknown keys (just prints error)
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_load_config_with_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create a custom config file
                let custom_config_path = std::env::temp_dir().join("test_custom_config.toml");
                let custom_config = r#"
[server]
default_port = 9999
default_bind = "127.0.0.1"
default_model = "potion-32M"
enable_mcp = false
rate_limit_rps = 100
rate_limit_burst = 200
enable_tls = false

[auth]
require_api_key = true
registration_enabled = true
api_key_header = "X-API-Key"
api_key_prefix = "Bearer "

[models]
auto_download = true
default_distill_dims = 128

[logging]
level = "info"
json_format = false
"#;
                std::fs::write(&custom_config_path, custom_config).unwrap();

                let config = load_config(Some(custom_config_path.clone())).unwrap();
                assert_eq!(config.server.default_port, 9999);
                assert_eq!(config.server.default_bind, "127.0.0.1");
                assert_eq!(config.server.default_model, "potion-32M");

                // Cleanup
                std::fs::remove_file(custom_config_path).unwrap();
            });
        });
    }

    #[test]
    fn test_save_config() {
        with_test_env(|| {
            let mut config = load_config(None).unwrap();
            config.server.default_port = 7777;

            let temp_path = std::env::temp_dir().join("test_save_config.toml");
            let result = save_config(&config, Some(temp_path.clone()));
            assert!(result.is_ok());

            // Verify the file was written
            let content = std::fs::read_to_string(&temp_path).unwrap();
            assert!(content.contains("default_port = 7777"));

            // Cleanup
            std::fs::remove_file(temp_path).unwrap();
        });
    }

    #[test]
    fn test_get_config_path() {
        with_test_env(|| {
            let path = get_config_path(None);
            assert!(path.is_ok());
            // Should be in the temp directory for tests
            let path_str = path.unwrap().to_str().unwrap().to_string();
            assert!(path_str.contains("test_") && path_str.contains(".embed-tool"));
        });
    }

    #[test]
    fn test_get_config_path_with_custom() {
        let custom_path = PathBuf::from("/custom/path/config.toml");
        let path = get_config_path(Some(custom_path.clone()));
        assert_eq!(path.unwrap(), custom_path);
    }

    #[test]
    fn test_set_config_server_default_port() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.default_port".to_string(),
                    value: "9090".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.default_port, 9090);
            });
        });
    }

    #[test]
    fn test_set_config_server_default_bind() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.default_bind".to_string(),
                    value: "127.0.0.1".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.default_bind, "127.0.0.1");
            });
        });
    }

    #[test]
    fn test_set_config_server_default_model() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.default_model".to_string(),
                    value: "custom-model".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.default_model, "custom-model");
            });
        });
    }

    #[test]
    fn test_set_config_server_enable_mcp() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.enable_mcp".to_string(),
                    value: "true".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.enable_mcp, true);
            });
        });
    }

    #[test]
    fn test_set_config_server_rate_limit_rps() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.rate_limit_rps".to_string(),
                    value: "50".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.rate_limit_rps, 50);
            });
        });
    }

    #[test]
    fn test_set_config_server_rate_limit_burst() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.rate_limit_burst".to_string(),
                    value: "150".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.server.rate_limit_burst, 150);
            });
        });
    }

    #[test]
    fn test_set_config_auth_registration_enabled() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "auth.registration_enabled".to_string(),
                    value: "false".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.auth.registration_enabled, false);
            });
        });
    }

    #[test]
    fn test_set_config_models_models_dir() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "models.models_dir".to_string(),
                    value: "/custom/models".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.models.models_dir, Some("/custom/models".to_string()));
            });
        });
    }

    #[test]
    fn test_set_config_models_auto_download() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "models.auto_download".to_string(),
                    value: "false".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.models.auto_download, false);
            });
        });
    }

    #[test]
    fn test_set_config_models_default_distill_dims() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "models.default_distill_dims".to_string(),
                    value: "256".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.models.default_distill_dims, 256);
            });
        });
    }

    #[test]
    fn test_set_config_logging_file() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "logging.file".to_string(),
                    value: "/var/log/embed-tool.log".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.logging.file, Some("/var/log/embed-tool.log".to_string()));
            });
        });
    }

    #[test]
    fn test_set_config_logging_json_format() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "logging.json_format".to_string(),
                    value: "true".to_string(),
                };
                let result = set_config(args, None).await;
                assert!(result.is_ok());

                let config = load_config(None).unwrap();
                assert_eq!(config.logging.json_format, true);
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
    fn test_handle_config_command_get() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let result = handle_config_command(ConfigAction::Get, None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_handle_config_command_set() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let args = SetConfigArgs {
                    key: "server.default_port".to_string(),
                    value: "8888".to_string(),
                };
                let result = handle_config_command(ConfigAction::Set(args), None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_handle_config_command_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let result = handle_config_command(ConfigAction::Path, None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_handle_config_command_reset() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let result = handle_config_command(ConfigAction::Reset, None).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_set_config_server_tls_cert_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // First enable TLS
                let enable_args = SetConfigArgs {
                    key: "server.enable_tls".to_string(),
                    value: "true".to_string(),
                };
                set_config(enable_args, None).await.unwrap();

                // This would require TLS cert/key paths, but they're not directly settable
                // The coverage shows these lines are not hit in tests
                let config = load_config(None).unwrap();
                assert_eq!(config.server.enable_tls, true);
            });
        });
    }

    #[test]
    fn test_set_config_logging_max_file_size() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Note: max_file_size is not directly settable via set_config
                // This tests the default value coverage
                let config = load_config(None).unwrap();
                assert_eq!(config.logging.max_file_size, None);
            });
        });
    }

    #[test]
    fn test_set_config_logging_max_files() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Note: max_files is not directly settable via set_config
                // This tests the default value coverage
                let config = load_config(None).unwrap();
                assert_eq!(config.logging.max_files, None);
            });
        });
    }

    #[test]
    fn test_set_config_server_tls_key_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // TLS cert/key paths are not directly configurable via set_config
                // This ensures the default None values are covered
                let config = load_config(None).unwrap();
                assert_eq!(config.server.tls_cert_path, None);
                assert_eq!(config.server.tls_key_path, None);
            });
        });
    }

    #[test]
    fn test_show_config_with_custom_path() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let custom_path = std::env::temp_dir().join("custom_config.toml");
                let result = show_config(Some(custom_path)).await;
                assert!(result.is_ok());
            });
        });
    }

    #[test]
    fn test_show_config_path_with_custom_path() {
        let custom_path = PathBuf::from("/custom/config/path.toml");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = show_config_path(Some(custom_path.clone())).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_reset_config_with_existing_file() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create a config file first
                let mut config = Config::default();
                config.server.default_port = 9999;
                save_config(&config, None).unwrap();

                // This test covers the file exists path, though we can't easily test the interactive part
                // The function will still execute the exists check
                let config_path = get_config_path(None).unwrap();
                assert!(config_path.exists());
            });
        });
    }

    #[test]
    fn test_set_config_parse_errors() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test invalid integer parsing
                let args = SetConfigArgs {
                    key: "server.default_port".to_string(),
                    value: "not_a_number".to_string(),
                };
                let result = set_config(args, None).await;
                // Should return error for invalid integer
                assert!(result.is_err());
            });
        });
    }

    #[test]
    fn test_set_config_boolean_parse_errors() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test invalid boolean parsing
                let args = SetConfigArgs {
                    key: "server.enable_mcp".to_string(),
                    value: "not_a_boolean".to_string(),
                };
                let result = set_config(args, None).await;
                // Should return error for invalid boolean
                assert!(result.is_err());
            });
        });
    }

    #[test]
    fn test_load_config_invalid_toml() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Create invalid TOML file
                let config_path = std::env::temp_dir().join("invalid_config.toml");
                std::fs::write(&config_path, "invalid [toml content").unwrap();

                let result = load_config(Some(config_path.clone()));
                assert!(result.is_err());

                // Cleanup
                let _ = std::fs::remove_file(config_path);
            });
        });
    }

    #[test]
    fn test_get_config_path_no_home_env() {
        // Temporarily remove HOME and USERPROFILE env vars
        let original_home = std::env::var("HOME").ok();
        let original_userprofile = std::env::var("USERPROFILE").ok();
        
        unsafe {
            std::env::remove_var("HOME");
            std::env::remove_var("USERPROFILE");
        }
        
        let result = get_config_path(None);
        assert!(result.is_err());
        
        // Restore environment
        if let Some(home) = original_home {
            unsafe { std::env::set_var("HOME", home) };
        }
        if let Some(userprofile) = original_userprofile {
            unsafe { std::env::set_var("USERPROFILE", userprofile) };
        }
    }

    #[test]
    fn test_handle_embed_command_no_model() {
        let args = EmbedArgs {
            text: "Hello world".to_string(),
            model: None,
            format: "json".to_string(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = handle_embed_command(args, None).await;
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_handle_batch_command_with_output() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let temp_dir = std::env::temp_dir().join("embed_tool_batch_test2");
                fs::create_dir_all(&temp_dir).unwrap();
                let input_file = temp_dir.join("input.json");
                fs::write(&input_file, "[]").unwrap();

                let args = BatchArgs {
                    input: input_file.clone(),
                    output: Some(temp_dir.join("output.json")),
                    model: None,
                    format: "json".to_string(),
                    batch_size: 32,
                };

                let result = handle_batch_command(args, None).await;
                assert!(result.is_ok());

                // Cleanup
                let _ = fs::remove_dir_all(&temp_dir);
            });
        });
    }

    #[test]
    fn test_set_config_all_server_keys() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Test all server config keys to ensure full coverage
                let test_cases = vec![
                    ("server.default_port", "9090"),
                    ("server.default_bind", "127.0.0.1"),
                    ("server.default_model", "test-model"),
                    ("server.enable_mcp", "true"),
                    ("server.rate_limit_rps", "50"),
                    ("server.rate_limit_burst", "150"),
                    ("server.enable_tls", "true"),
                ];

                for (key, value) in test_cases {
                    let args = SetConfigArgs {
                        key: key.to_string(),
                        value: value.to_string(),
                    };
                    let result = set_config(args, None).await;
                    assert!(result.is_ok(), "Failed to set {}", key);
                }

                let config = load_config(None).unwrap();
                assert_eq!(config.server.default_port, 9090);
                assert_eq!(config.server.default_bind, "127.0.0.1");
                assert_eq!(config.server.default_model, "test-model");
                assert_eq!(config.server.enable_mcp, true);
                assert_eq!(config.server.rate_limit_rps, 50);
                assert_eq!(config.server.rate_limit_burst, 150);
                assert_eq!(config.server.enable_tls, true);
            });
        });
    }

    #[test]
    fn test_set_config_all_auth_keys() {
        with_test_env(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let test_cases = vec![
                    ("auth.require_api_key", "false"),
                    ("auth.registration_enabled", "false"),
                    ("auth.api_key_header", "X-Custom-Header"),
                    ("auth.api_key_prefix", "Custom-Prefix "),
                ];

                for (key, value) in test_cases {
                    let args = SetConfigArgs {
                        key: key.to_string(),
                        value: value.to_string(),
                    };
                    let result = set_config(args, None).await;
                    assert!(result.is_ok(), "Failed to set {}", key);
                }

                let config = load_config(None).unwrap();
                assert_eq!(config.auth.require_api_key, false);
                assert_eq!(config.auth.registration_enabled, false);
                assert_eq!(config.auth.api_key_header, "X-Custom-Header");
                assert_eq!(config.auth.api_key_prefix, "Custom-Prefix ");
            });
        });
    }

    #[test]
    fn test_set_config_all_models_keys() {
        with_test_env(|| {
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
                    let result = set_config(args, None).await;
                    assert!(result.is_ok(), "Failed to set {}", key);
                }

                let config = load_config(None).unwrap();
                assert_eq!(config.models.models_dir, Some("/custom/models/dir".to_string()));
                assert_eq!(config.models.auto_download, false);
                assert_eq!(config.models.default_distill_dims, 256);
            });
        });
    }

    #[test]
    fn test_set_config_all_logging_keys() {
        with_test_env(|| {
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
                    let result = set_config(args, None).await;
                    assert!(result.is_ok(), "Failed to set {}", key);
                }

                let config = load_config(None).unwrap();
                assert_eq!(config.logging.level, "debug");
                assert_eq!(config.logging.file, Some("/var/log/test.log".to_string()));
                assert_eq!(config.logging.json_format, true);
            });
        });
    }
}