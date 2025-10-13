use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;

mod server;
mod models;
mod config;

pub use server::*;
pub use models::*;
pub use config::*;

#[derive(Parser)]
#[command(name = "embed-tool")]
#[command(about = "Static embedding server with Model2Vec integration")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
    
    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Server management commands
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
    /// Model management commands
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Quick embedding operations
    Embed(EmbedArgs),
    /// Batch embedding operations
    Batch(BatchArgs),
}

#[derive(Subcommand)]
pub enum ServerAction {
    /// Start the embedding server (HTTP API and MCP)
    Start(StartArgs),
    /// Stop the running server
    Stop,
    /// Get server status
    Status,
    /// Restart the server
    Restart(StartArgs),
}

#[derive(Args)]
pub struct StartArgs {
    /// Port to bind the HTTP server
    #[arg(long, default_value = "8080")]
    pub port: u16,

    /// Bind address
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: String,

    /// Unix socket path (mutually exclusive with bind)
    #[arg(long, conflicts_with = "bind")]
    pub socket_path: Option<PathBuf>,

    /// Models to load (comma-separated)
    #[arg(long, value_parser = validate_models)]
    pub models: Option<String>,

    /// Default model to use
    #[arg(long, default_value = "potion-32M", value_parser = validate_model_name)]
    pub default_model: String,

    /// Enable MCP mode alongside HTTP
    #[arg(long)]
    pub mcp: bool,

    /// Disable authentication
    #[arg(long)]
    pub auth_disabled: bool,

    /// Run as daemon (detached process)
    #[arg(long)]
    pub daemon: bool,

    /// PID file location for daemon mode
    #[arg(long)]
    pub pid_file: Option<PathBuf>,

    /// TLS certificate file path
    #[arg(long)]
    pub tls_cert_path: Option<String>,

    /// TLS private key file path
    #[arg(long)]
    pub tls_key_path: Option<String>,
}/// Validate models string: comma-separated non-empty names
fn validate_models(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        return Err("Models list cannot be empty".to_string());
    }
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        Err("No valid models found in list".to_string())
    } else {
        Ok(())
    }
}

/// Validate model name: non-empty
fn validate_model_name(s: &str) -> Result<(), String> {
    if s.trim().is_empty() {
        Err("Model name cannot be empty".to_string())
    } else {
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum ModelAction {
    /// List available models
    List,
    /// Download a pre-trained model
    Download(DownloadArgs),
    /// Distill a custom model
    Distill(DistillArgs),
    /// Remove a model
    Remove(RemoveArgs),
    /// Update/refresh a model
    Update(UpdateArgs),
    /// Show model information
    Info(InfoArgs),
}

#[derive(Args)]
pub struct DownloadArgs {
    /// Model name or HuggingFace model ID
    pub model_name: String,
    
    /// Local name/alias for the model
    #[arg(short, long)]
    pub alias: Option<String>,
    
    /// Force redownload if exists
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct DistillArgs {
    /// Input model name or path
    pub input: String,
    
    /// Output model name/path
    pub output: String,
    
    /// PCA dimensions for distillation
    #[arg(short, long, default_value = "128")]
    pub dims: usize,
    
    /// Force overwrite if output exists
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// Model name to remove
    pub model_name: String,
    
    /// Remove without confirmation
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// Model name to update
    pub model_name: String,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Model name to inspect
    pub model_name: String,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Get,
    /// Set a configuration value
    Set(SetConfigArgs),
    /// Reset configuration to defaults
    Reset,
    /// Show configuration file location
    Path,
}

#[derive(Args)]
pub struct SetConfigArgs {
    /// Configuration key (e.g., auth.require_api_key)
    pub key: String,
    
    /// Configuration value
    pub value: String,
}

#[derive(Args)]
pub struct EmbedArgs {
    /// Text to embed
    pub text: String,
    
    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,
    
    /// Output format (json, csv, raw)
    #[arg(short, long, default_value = "json")]
    pub format: String,
}

#[derive(Args)]
pub struct BatchArgs {
    /// Input file (JSON array of strings)
    pub input: PathBuf,
    
    /// Output file
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    
    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,
    
    /// Output format (json, csv, npy)
    #[arg(short, long, default_value = "json")]
    pub format: String,
    
    /// Batch size for processing
    #[arg(short, long, default_value = "32")]
    pub batch_size: usize,
}

pub async fn run_cli() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging based on verbosity
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }
    
    match cli.command {
        Commands::Server { action } => {
            handle_server_command(action, cli.config).await.map_err(Into::into)
        }
        Commands::Model { action } => {
            handle_model_command(action, cli.config).await.map_err(Into::into)
        }
        Commands::Config { action } => {
            handle_config_command(action, cli.config).await.map_err(Into::into)
        }
        Commands::Embed(args) => {
            handle_embed_command(args, cli.config).await.map_err(Into::into)
        }
        Commands::Batch(args) => {
            handle_batch_command(args, cli.config).await.map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_validate_models_valid() {
        assert!(validate_models("model1,model2,model3").is_ok());
        assert!(validate_models("model1").is_ok());
        assert!(validate_models("  model1  ,  model2  ").is_ok());
    }

    #[test]
    fn test_validate_models_invalid() {
        assert!(validate_models("").is_err());
        assert!(validate_models("   ").is_err());
        assert!(validate_models(",,,,").is_err());
    }

    #[test]
    fn test_validate_model_name_valid() {
        assert!(validate_model_name("model1").is_ok());
        assert!(validate_model_name("my-model").is_ok());
        assert!(validate_model_name("model_123").is_ok());
    }

    #[test]
    fn test_validate_model_name_invalid() {
        assert!(validate_model_name("").is_err());
        assert!(validate_model_name("   ").is_err());
    }

    #[test]
    fn test_cli_parsing_server_start() {
        let args = vec!["embed-tool", "server", "start", "--port", "9090", "--bind", "127.0.0.1"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Server { action } => {
                match action {
                    ServerAction::Start(args) => {
                        assert_eq!(args.port, 9090);
                        assert_eq!(args.bind, "127.0.0.1");
                        assert_eq!(args.default_model, "potion-32M");
                        assert!(!args.mcp);
                        assert!(!args.auth_disabled);
                        assert!(!args.daemon);
                    }
                    _ => panic!("Expected Start action"),
                }
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_parsing_server_start_with_models() {
        let args = vec![
            "embed-tool",
            "server",
            "start",
            "--models",
            "model1,model2,model3",
            "--default-model",
            "model2",
            "--mcp",
            "--auth-disabled"
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Server { action } => {
                match action {
                    ServerAction::Start(args) => {
                        assert_eq!(args.models, Some("model1,model2,model3".to_string()));
                        assert_eq!(args.default_model, "model2");
                        assert!(args.mcp);
                        assert!(args.auth_disabled);
                    }
                    _ => panic!("Expected Start action"),
                }
            }
            _ => panic!("Expected Server command"),
        }
    }

    #[test]
    fn test_cli_parsing_embed() {
        let args = vec![
            "embed-tool",
            "embed",
            "Hello world",
            "--model",
            "custom-model",
            "--format",
            "csv"
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Embed(args) => {
                assert_eq!(args.text, "Hello world");
                assert_eq!(args.model, Some("custom-model".to_string()));
                assert_eq!(args.format, "csv");
            }
            _ => panic!("Expected Embed command"),
        }
    }

    #[test]
    fn test_cli_parsing_batch() {
        let args = vec![
            "embed-tool",
            "batch",
            "/path/to/input.json",
            "--output",
            "/path/to/output.json",
            "--model",
            "batch-model",
            "--batch-size",
            "64"
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.command {
            Commands::Batch(args) => {
                assert_eq!(args.input, std::path::PathBuf::from("/path/to/input.json"));
                assert_eq!(args.output, Some(std::path::PathBuf::from("/path/to/output.json")));
                assert_eq!(args.model, Some("batch-model".to_string()));
                assert_eq!(args.batch_size, 64);
                assert_eq!(args.format, "json");
            }
            _ => panic!("Expected Batch command"),
        }
    }

    #[test]
    fn test_cli_parsing_model_actions() {
        // Test model list
        let args = vec!["embed-tool", "model", "list"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Model { action: ModelAction::List } => {}
            _ => panic!("Expected Model List action"),
        }

        // Test model download
        let args = vec!["embed-tool", "model", "download", "model-name", "--alias", "my-model"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Model { action: ModelAction::Download(args) } => {
                assert_eq!(args.model_name, "model-name");
                assert_eq!(args.alias, Some("my-model".to_string()));
                assert!(!args.force);
            }
            _ => panic!("Expected Model Download action"),
        }
    }

    #[test]
    fn test_cli_parsing_config_actions() {
        // Test config get
        let args = vec!["embed-tool", "config", "get"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Config { action: ConfigAction::Get } => {}
            _ => panic!("Expected Config Get action"),
        }

        // Test config set
        let args = vec!["embed-tool", "config", "set", "key", "value"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Config { action: ConfigAction::Set(args) } => {
                assert_eq!(args.key, "key");
                assert_eq!(args.value, "value");
            }
            _ => panic!("Expected Config Set action"),
        }
    }

    #[test]
    fn test_cli_global_flags() {
        let args = vec!["embed-tool", "--config", "/path/to/config.toml", "--verbose", "server", "status"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.config, Some(std::path::PathBuf::from("/path/to/config.toml")));
        assert!(cli.verbose);
    }
}
