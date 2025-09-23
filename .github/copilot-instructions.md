---
description: Static embedding server with Model2Vec integration and MCP capabilities
globs: *
---

## PROJECT OVERVIEW

This is a **static embedding server** built with Rust, providing HTTP API and MCP (streaming http) for Model2Vec embeddings in OpenAI-compatible format, and a CLI interface to manage and configure the server.

**Note**: The codebase has been successfully transformed from SurrealDB MCP server components to a dedicated embedding server. The transformation maintained proven patterns from the SurrealDB project including authentication, rate limiting, CLI design, and MCP integration while adapting them for embedding-specific functionality.

**Core Focus**: HTTP API and MCP serving multiple Model2Vec embedding models with graceful fallbacks and OpenAI API compatibility.

### Successfully Adapted Patterns

The following components have been successfully adapted from SurrealDB patterns:

- **CLI Structure**: `src/cli/` implements embedding-specific commands (model distillation, server management, config)
- **Authentication**: JWT/JWE patterns with JWKS adapted for embedding API security
- **Rate Limiting**: IP-based rate limiting patterns protect embedding API endpoints
- **MCP Integration**: Resource providers and tool implementations serve embedding documentation and utilities
- **Server Architecture**: Axum + tower middleware stack provides robust foundation for HTTP + MCP modes
- **Error Handling**: EmbeddingError patterns adapted from McpError with similar structure

**Take Ownership Of Tool Issues**: Always assume you are at fault first, and something you did that has caused any issues with a tool being used or command being executed. The tools have been working, and most of the time it is your issue. You didn't wait long enough, you didn't read the instructions, you didn't follow the steps correctly, you didn't install something properly, etc. Review your steps and actions carefully, read the instructions again, find existence of proof it something else before blaming the tool or code. Absence is not proof, so you can not say "it doesn't work" or "it is broken" just because you can't get it to work. You are likely doing something wrong. Take ownership of your mistakes first, and only after you have exhausted all possibilities and confirmed it is not your fault, then consider it might be a tool issue.

example:
a. executed a terminal command and you have not seen a response. Assumption - Your at Fault. You have not identified existence of proof it is not your fault.
b. The terminal shows your command and the following prompt waiting for input. Assumption - Your Fault. You have identified existence of potential evidence, but it may be circumstantial or not really proof, so the presumption is still you are at fault.
c. The terminal shows an error message with a stacktrace that indicates a bug in the tool or code. Assumption - Not Your Fault. You have identified existence of proof it is not your fault.

## ARCHITECTURE

### Core Components

- **`src/main.rs`** - CLI entry point, delegates all functionality to the CLI module
- **`src/cli/`** - Complete CLI implementation with subcommands for server, model, and config management
- **`src/server/mod.rs`** - HTTP server implementation with Axum, loads multiple Model2Vec models (`potion-8M`, `potion-32M`, `code-distilled`)
- **`src/utils/mod.rs`** - Utility functions including model distillation via external `model2vec` Python CLI
- **`src/server/`** - HTTP server infrastructure (auth, rate limiting, health checks)
- **`src/logs/mod.rs`** - Structured logging and metrics initialization with tracing-subscriber
- **`src/resources/mod.rs`** - MCP resource provider system for serving documentation
- **`src/tools/`** - MCP tool implementations providing patterns for embedding-related utilities
- **`src/cli/`** - CLI interface module to be implemented with embedding-specific commands (model management, distillation, server config)

### Key Patterns

- **Multi-model support**: AppState maintains HashMap of loaded StaticModel instances
- **OpenAI-compatible API**: `/v1/embeddings` endpoint matches OpenAI embedding API format
- **Graceful fallbacks**: Auto-versioning for file conflicts, multiple model loading attempts
- **Cross-platform path handling**: HOME/USERPROFILE detection for macOS/Windows

## TECH STACK

- **Web Framework**: Axum with tower middleware stack
- **ML Models**: model2vec-rs StaticModel for embeddings
- **Authentication**: JWT/JWE bearer token validation with JWKS (adapted from SurrealDB project patterns)
- **Rate Limiting**: tower_governor with IP-based extraction
- **Logging**: tracing with structured logs and metrics via tracing-subscriber
- **Docker Support**: Multi-stage builds with chainguard base images
- **Inherited Infrastructure**: Embedding-focused patterns adapted from SurrealDB project, MCP protocol (rmcp) - providing proven patterns for auth, rate limiting, and server architecture

## CLI IMPLEMENTATION GOALS

The `src/cli/` module implements a comprehensive embedding-focused command-line interface that manages the entire server lifecycle. **Only one instance should ever run at a time**, eliminating the need for stdio handling. The CLI is the primary interface for all operations.

### Core CLI Commands

```bash
# Server lifecycle management (primary function)
embed-tool server start --port 8080 --models potion-32M,code-distilled  # Start HTTP API and MCP server
embed-tool server stop                     # Stop the running server
embed-tool server status                   # Check server status
embed-tool server restart                  # Stop and start

# Model management and distillation
embed-tool model list                      # List available models
embed-tool model download <name>           # Download pre-trained model
embed-tool model distill <input> <output> --dims 128  # Distill custom model
embed-tool model remove <name>             # Remove model
embed-tool model info <name>               # Show model details

# Configuration management
embed-tool config set auth.jwks_url https://auth.example.com/.well-known/jwks.json
embed-tool config get                      # Show current configuration

# Quick operations (server must be running)
embed-tool embed "Hello world" --model potion-32M  # Quick embedding test
embed-tool batch embeddings.json --output results.json  # Batch processing
```

### CLI Architecture Implementation

- **Single Instance Control**: PID file management ensures only one server runs
- **Daemon Mode**: `--daemon` flag for background operation with process management
- **Process Lifecycle**: Start/stop/restart commands with proper cleanup and status checking
- **Configuration Management**: TOML config files with environment variable overrides
- **Model Registry**: JSON-based registry tracking downloaded and distilled models
- **Command Structure**: Follows proven CLI subcommand pattern (`tool <noun> <verb>`) adapted from SurrealDB design
- **Error Handling**: Structured error messages with helpful suggestions and troubleshooting

## DEVELOPMENT WORKFLOW

### Building & Running

```bash
# Build and run embedding server
cargo run  # Starts on 0.0.0.0:8080

# Available models: potion-8M, potion-32M, code-distilled (if present)
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"input": ["Hello world"], "model": "potion-32M"}'

# Docker build and run
docker build -t static-embed-tool .
docker run -p 8080:8080 static-embed-tool
```

### Testing Patterns

- Unit tests in modules with `#[cfg(test)]`
- Integration tests for HTTP endpoints and auth middleware
- GitHub Actions CI with format, clippy, test, and doc checks
- Multi-arch Docker builds (amd64/arm64) via GitHub Actions

### Model Distillation

The `utils::distill()` function calls external Python `model2vec` CLI:

```bash
python -c "
import model2vec
model = model2vec.StaticModel.from_pretrained('model_name')
distilled = model.distill(pca_dims=128)
distilled.save('output_path')
"
```

## CONFIGURATION

### Authentication Configuration

- Bearer token validation with JWKS endpoint: `https://auth.embed.example.com/.well-known/jwks.json`
- OAuth discovery at `/.well-known/oauth-protected-resource`
- Custom audience configuration via `auth_audience` parameter
- API token security for embedding endpoints and MCP endpoints (standard MCP practice)

## CRITICAL IMPLEMENTATION DETAILS

### Model Loading Strategy

- Loads multiple models at startup in `AppState::new()`
- Fallback model loading: if custom distilled model fails, continues with base models
- Model selection via request parameter or defaults to `potion-32M`

### Path Resolution

- Cross-platform home directory: `env::var("HOME").or_else(|_| env::var("USERPROFILE"))`
- Auto-versioning prevents overwriting: `model_v2`, `model_v3`, etc.
- Directory creation with `fs::create_dir_all()` for missing paths

### Error Handling Patterns

- Result types with `Box<dyn std::error::Error>` for main functions
- McpError for MCP protocol errors with internal_error wrapper (may be adapted for embedding errors)
- Structured logging with context via tracing spans

### Rate Limiting

- IP-based extraction with fallback to "unknown" for missing headers
- Configurable RPS and burst limits via `create_rate_limit_layer()`
- Metrics integration with `counter!()` macros

## DEBUGGING

### Key Log Messages

```bash
# Server startup
"Starting MCP server in HTTP mode with rate limiting"
"Model2Vec-rs server running on http://0.0.0.0:8080"

# Model operations
"Distilling model 'X' with Y PCA dimensions..."
"✓ Model distilled successfully to: /path"
"⚠️ File exists, saving as: model_v2"
```

### Common Issues

- Python `model2vec` not in PATH - function tries multiple execution methods
- Model loading failures - check if model exists and is accessible
- Authentication errors - verify JWKS endpoint accessibility
- API token validation failures - check bearer token format and validity
