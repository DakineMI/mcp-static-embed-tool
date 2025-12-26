<br>

<p align="center">
    <img width=120 src="https://raw.githubusercontent.com/dakinemi/icons/main/embed.svg" />
    &nbsp;
    <img width=120 src="https://raw.githubusercontent.com/surrealdb/icons/main/mcp.svg" />
</p>

<h3 align="center">Static embedding server with Model2Vec integration and MCP capabilities.</h3>

<br>

<p align="center">
    <a href="https://github.com/dakinemi/static-embedding-tool"><img src="https://img.shields.io/badge/status-preview-ff00bb.svg?style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/dakinemi/static-embedding-tool"><img src="https://img.shields.io/github/v/release/dakinemi/static-embedding-tool?color=9600FF&include_prereleases&label=version&sort=semver&style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/dakinemi/static-embedding-tool/blob/main/server.md"><img src="https://img.shields.io/badge/docs-view-44cc11.svg?style=flat-square"></a>
    &nbsp;
    <a href="https://github.com/dakinemi/static-embedding-tool/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-BSL_1.1-00bfff.svg?style=flat-square"></a>
</p>

# Static Embedding Server

Static Embedding Server is a high-performance Rust-based embedding server that provides OpenAI-compatible HTTP API and MCP (Model Context Protocol) integration for Model2Vec embeddings. It features a comprehensive CLI for server management, model operations, and configuration.

### Model Distillation

You can distill existing models to create smaller, faster versions. The tool automatically adjusts default PCA dimensions based on the input model name (e.g., 32 for "32M" models, 8 for "8M" models) if not explicitly provided.

```bash
static-embedding-tool model distill minishlab/potion-base-8M mini-model
```

You can also specify dimensions manually:

```bash
static-embedding-tool model distill sentence-transformers/all-MiniLM-L6-v2 custom-model --dims 32
```

## Features

- **CLI-first architecture**: Complete server lifecycle management through intuitive commands
- **Multi-model support**: Load multiple Model2Vec models simultaneously (`potion-8M`, `potion-32M`, `code-distilled`)
- **OpenAI-compatible API**: `/v1/embeddings` endpoint matching OpenAI embedding API format
- **Model distillation**: Built-in support for custom model creation via Model2Vec distillation
- **Single instance control**: PID file-based process management ensuring only one server runs
- **Health checks**: Built-in health monitoring and status endpoints
- **Structured logging**: Comprehensive logging and metrics with tracing-subscriber
- **MCP integration**: Model Context Protocol support for AI assistant integration
- **Configuration management**: TOML-based hierarchical configuration with environment overrides
- **Cross-platform**: Support for macOS, Linux, and Windows with proper path resolution

## Installation

### As a Library

Add this to your `Cargo.toml` to use it as a fast, lightweight embedding library without the server dependencies:

```toml
[dependencies]
static-embedding-tool = { version = "1.0.0", default-features = false }
```

### Building from source

```bash
git clone https://github.com/dakinemi/static-embedding-tool.git
cd static-embedding-tool
cargo build --release
cargo install --path .
```

### Using Docker

```bash
# Build the Docker image
docker build -t static-embedding-tool .

# Run with default settings
docker run --rm -p 8084:8084 static-embedding-tool server start

# Run with custom configuration
docker run --rm -p 8084:8084 -v $(pwd)/config.toml:/app/config.toml static-embedding-tool server start --config /app/config.toml
```

## Quick Start

### Library Usage

You can use the `Embedder` directly in your Rust projects:

```rust
use static_embedding_tool::Embedder;

fn main() -> anyhow::Result<()> {
    // Load model (downloads automatically if needed)
    let embedder = Embedder::new("potion-32M")?;

    // Generate embedding
    let embedding = embedder.embed("Hello, world!");
    println!("Dimensions: {}", embedding.len());

    // Batch embedding
    let inputs = vec!["Hello".to_string(), "World".to_string()];
    let embeddings = embedder.embed_batch(&inputs);
    
    Ok(())
}
```

### CLI Usage

The embedding server is managed entirely through the CLI interface:

```bash
# Start the embedding server
static-embedding-tool server start --port 8084 --models potion-32M,code-distilled

# Check server status
static-embedding-tool server status

# Get embeddings for text
static-embedding-tool embed "Hello, world!" --model potion-32M

# Batch process embeddings
static-embedding-tool batch input.json --output results.json

# List available models
static-embedding-tool model list

# Distill a custom model
static-embedding-tool model distill sentence-transformers/all-MiniLM-L6-v2 custom-model --dims 32

# Stop the server
static-embedding-tool server stop
```

### HTTP API Usage

Once the server is running, you can use the OpenAI-compatible embeddings endpoint:

```bash
curl -X POST http://localhost:8084/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "input": ["Hello, world!", "How are you?"],
    "model": "potion-32M"
  }'
```

### Available Models

The server supports multiple Model2Vec models:

- **`potion-8M`** - Fast, lightweight model for quick embeddings
- **`potion-32M`** - Balanced performance and quality (default)
- **`code-distilled`** - Specialized for code embeddings (if available)
- **Custom models** - Distilled models via `static-embedding-tool model distill`

## Configuration

### CLI Configuration

The embedding server uses TOML-based configuration with environment variable overrides:

```bash
# Set configuration values
static-embedding-tool config set server.port 8084
static-embedding-tool config set server.host "127.0.0.1"
static-embedding-tool config set models.default "potion-32M"

# View current configuration
static-embedding-tool config get

# Use custom config file
static-embedding-tool server start --config /path/to/config.toml
```

### Environment Variables

All configuration can be overridden with environment variables:

```bash
# Server settings
export EMBED_TOOL_SERVER_PORT=8084
export EMBED_TOOL_SERVER_HOST="127.0.0.1"

# Models
export EMBED_TOOL_MODELS_DEFAULT="potion-32M"
export EMBED_TOOL_MODELS_PATH="/custom/models/path"

### Configuration File Format

Example `config.toml`:

```toml
[server]
port = 8084
host = "127.0.0.1"
workers = 4

[models]
default = "potion-32M"
available = ["potion-8M", "potion-32M", "code-distilled"]
path = "/opt/models"

[logging]
level = "info"
format = "json"
```

## AI Tools Integration

The Static Embedding Server provides MCP (Model Context Protocol) integration for AI assistants and development tools. This enables AI systems to access embedding capabilities through a standardized protocol.

### Supported AI Tools

- **VS Code Extensions**: Cline, GitHub Copilot, Roo Code
- **IDEs**: Cursor, Windsurf, Zed
- **Desktop Applications**: Claude Desktop
- **Automation Platforms**: n8n, custom integrations

### MCP Integration

Configure your AI tool to connect to the embedding server via MCP:

```json
{
  "mcpServers": {
    "static-embedding-tool": {
      "command": "static-embedding-tool",
      "args": ["server", "start", "--mcp"]
    }
  }
}
```

## API Reference

### HTTP Endpoints

#### Embeddings Endpoint

**POST** `/v1/embeddings`

Generate embeddings for input text using OpenAI-compatible API format.

**Request Body:**
```json
{
  "input": ["Hello, world!", "How are you?"],
  "model": "potion-32M",
  "encoding_format": "float"
}
```

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "embedding": [0.1, 0.2, -0.3, ...],
      "index": 0
    },
    {
      "object": "embedding", 
      "embedding": [0.4, -0.1, 0.8, ...],
      "index": 1
    }
  ],
  "model": "potion-32M",
  "usage": {
    "prompt_tokens": 6,
    "total_tokens": 6
  }
}
```

#### Health Check

**GET** `/health`

Returns server health status and loaded models.

**Response:**

```json
{
  "status": "healthy",
  "models": ["potion-8M", "potion-32M", "code-distilled"],
  "server_info": {
    "version": "0.1.0",
    "uptime": "2h 15m 30s"
  }
}
```

### Model Management

#### List Models

**GET** `/v1/models`

Returns available embedding models.

**Response:**

```json
{
  "object": "list",
  "data": [
    {
      "id": "potion-8M",
      "object": "model",
      "created": 1627846261,
      "owned_by": "model2vec",
      "dimensions": 256
    },
    {
      "id": "potion-32M", 
      "object": "model",
      "created": 1627846261,
      "owned_by": "model2vec",
      "dimensions": 256
    }
  ]
}
```

## CLI Commands

### Server Management

```bash
# Start server with specific models
static-embedding-tool server start --port 8084 --models potion-32M,code-distilled

# Start with authentication disabled (development only)
static-embedding-tool server start --auth-disabled

# Start in daemon mode
static-embedding-tool server start --daemon --log-file /var/log/static-embedding-tool.log

# Check server status
static-embedding-tool server status

# Stop server
static-embedding-tool server stop

# Restart server
static-embedding-tool server restart
```

### Model Operations

```bash
# List available models
static-embedding-tool model list

# Download a pre-trained model
static-embedding-tool model download potion-32M

# Distill a custom model
static-embedding-tool model distill sentence-transformers/all-MiniLM-L6-v2 custom-mini --dims 32

# Remove a model
static-embedding-tool model remove old-model

# Get model information
static-embedding-tool model info potion-32M
```

### Configuration Management

```bash
# Set configuration values
static-embedding-tool config set server.port 8084
static-embedding-tool config set auth.require_auth true
static-embedding-tool config set models.default potion-32M

# Get configuration
static-embedding-tool config get
static-embedding-tool config get server.port

# Reset to defaults
static-embedding-tool config reset
```

### Quick Operations

```bash
# Generate embeddings for text
static-embedding-tool embed "Hello, world!" --model potion-32M

# Batch process from file
static-embedding-tool batch input.jsonl --output results.jsonl --model code-distilled

# Test server connectivity
static-embedding-tool embed "test" --endpoint http://localhost:8084
```

## CLI Commands

## Development

### Building from Source

```bash
git clone https://github.com/dakinemi/static-embedding-tool.git
cd static-embedding-tool
cargo build --release
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test module
cargo test cli::tests

# Run integration tests
cargo test --test integration
```

### Docker Development

```bash
# Build development image
docker build -t static-embedding-tool:dev .

# Run with development settings
docker run --rm -p 8084:8084 -e RUST_LOG=debug static-embedding-tool:dev server start

# Mount local code for development
docker run --rm -p 8084:8084 -v $(pwd):/app static-embedding-tool:dev
```

## Troubleshooting

### Common Issues

**Server fails to start:**

- Check if port is available: `netstat -an | grep 8084`
- Verify model files exist: `static-embedding-tool model list`
- Check logs: `static-embedding-tool server status --verbose`

**Model loading errors:**

- Ensure sufficient memory for large models
- Verify model file integrity: `static-embedding-tool model info <model>`
- Check disk space for model storage

### Logging

Configure logging levels and formats:

```bash
# Set log level
export RUST_LOG=debug
static-embedding-tool server start

# JSON formatted logs
static-embedding-tool config set logging.format json

# Log to file
static-embedding-tool server start --log-file /var/log/static-embedding-tool.log
```

## Contributing

We welcome contributions! Please see our [contributing guidelines](CONTRIBUTING.md).

### Development Setup

1. Install Rust toolchain
2. Clone the repository  
3. Install dependencies: `cargo build`
4. Run tests: `cargo test`
5. Submit pull request

## License

This project is licensed under the [Business Source License 1.1](LICENSE).

For alternative licensing arrangements, please contact the maintainers.
