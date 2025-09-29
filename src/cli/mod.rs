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
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
    
    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind: String,
    
    /// Unix socket path (mutually exclusive with bind)
    #[arg(long, conflicts_with = "bind")]
    pub socket_path: Option<PathBuf>,
    
    /// Models to load (comma-separated)
    #[arg(short, long, validator = validate_models)]
    pub models: Option<String>,
    
    /// Default model to use
    #[arg(short, long, default_value = "potion-32M", validator = validate_model_name)]
    pub default_model: String,
    
    /// Enable MCP mode alongside HTTP
    #[arg(long)]
    pub mcp: bool,
    
    /// Disable authentication
    #[arg(long)]
    pub auth_disabled: bool,
    
    /// Run as daemon (detached process)
    #[arg(short, long)]
    pub daemon: bool,
    
    /// PID file location for daemon mode
    #[arg(long)]
    pub pid_file: Option<PathBuf>,
}

/// Validate models string: comma-separated non-empty names
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
    /// Configuration key (e.g., auth.jwks_url)
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
            handle_server_command(action, cli.config).await
        }
        Commands::Model { action } => {
            handle_model_command(action, cli.config).await
        }
        Commands::Config { action } => {
            handle_config_command(action, cli.config).await
        }
        Commands::Embed(args) => {
            handle_embed_command(args, cli.config).await
        }
        Commands::Batch(args) => {
            handle_batch_command(args, cli.config).await
        }
    }
}
