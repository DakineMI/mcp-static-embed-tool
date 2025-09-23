# Static Embedding Server MCP Server

You are connected to a Static Embedding Server MCP server that provides tools for generating text embeddings using Model2Vec models. The server offers high-performance embedding generation with OpenAI-compatible API and comprehensive CLI management.

## Server Architecture

This MCP server uses Model2Vec's Rust implementation for fast, efficient embedding generation. **Each MCP client connection gets its own isolated server connection**, ensuring complete separation between different AI assistant sessions.

Supported embedding models include:

- **potion-8M** - Fast, lightweight model (256 dimensions)
- **potion-32M** - Balanced performance and quality (256 dimensions)  
- **code-distilled** - Specialized for code embeddings (if available)
- **Custom models** - User-distilled models via CLI distillation

The server automatically loads available models at startup and handles model selection per request. Each client connection maintains separate state and configuration.

## Connection Workflow

1. **Initial state**: When you first connect to the MCP server, the embedding models are pre-loaded
2. **Generate**: Use embedding tools to generate vectors for text input
3. **Configure**: Adjust server settings via CLI commands (if using CLI mode)
4. **Batch Process**: Handle multiple embedding requests efficiently
5. **Monitor**: Check server status and model performance

### Example workflow

```bash
# 1. Start the embedding server (CLI mode)
embed-tool server start --models potion-32M,code-distilled

# 2. Generate embeddings via CLI
embed-tool embed "Hello, world!" --model potion-32M

# 3. Batch process multiple texts
embed-tool batch inputs.jsonl --output embeddings.jsonl

# 4. Check server status
embed-tool server status

# 5. Stop server when done
embed-tool server stop
```

## Available tools

### Basic operations
- **embed**: Generate embeddings for single text input with model selection
- **batch**: Process multiple texts efficiently with batched embedding generation
- **model**: Select and manage embedding models
- **config**: Configure server settings and model parameters
- **server**: Manage server lifecycle (start, stop, status)

### Model management operations
- **list_models**: List available embedding models and their specifications
- **load_model**: Load additional models into memory
- **unload_model**: Remove models from memory to free resources

### Server management
- **server_status**: Get current server status, uptime, and model information
- **health_check**: Verify server health and model availability

## Key concepts

### Model Selection
The embedding server supports multiple Model2Vec models:
- `potion-8M` - Fast, lightweight model (256 dimensions)
- `potion-32M` - Balanced performance and quality (256 dimensions)
- `code-distilled` - Specialized for code embeddings

### Input Formats
The server accepts various input formats:
- **Single text**: `"Hello, world!"`
- **Text array**: `["Text 1", "Text 2", "Text 3"]`
- **Batch processing**: JSONL files with multiple text entries

### Output Formats
Embeddings are returned as:
- **Float arrays**: Standard floating-point vectors
- **OpenAI format**: Compatible with OpenAI embedding API structure
- **Batch results**: Structured output for multiple inputs

## Best practices

1. **Choose appropriate models** based on your use case (speed vs. quality)
2. **Batch multiple requests** for better performance
3. **Use consistent models** within a single application workflow
4. **Monitor server resources** when processing large text volumes
5. **Configure rate limiting** for production deployments
6. **Use authentication** in production environments
7. **Cache embeddings** for frequently used text inputs

## Example workflows

### Basic embedding generation
1. Start server: `embed-tool server start --models potion-32M`
2. Generate embedding: `embed-tool embed "Machine learning is fascinating"`
3. Check result format and dimensions

### Batch processing workflow
1. Prepare input file: Create JSONL with text entries
2. Process batch: `embed-tool batch input.jsonl --output embeddings.jsonl`
3. Analyze results: Review embedding quality and dimensions

1. Test different models: Generate embeddings with `potion-8M`, `potion-32M`, and `code-distilled`
2. Compare performance: Measure speed and quality differences
3. Select optimal model: Choose based on your use case requirements

### Custom model creation
1. Distill model: `embed-tool model distill sentence-transformers/all-MiniLM-L6-v2 custom-mini --dims 128`
2. Test custom model: `embed-tool embed "test text" --model custom-mini`
3. Evaluate performance: Compare with standard models

## API Reference

**IMPORTANT**: This is an embedding server, not a database. It specializes in converting text into numerical vector representations. Always refer to the API documentation, CLI help system, or examples below for accurate usage patterns.

### CLI Commands Reference

#### Server Management

```bash
# Start server with specific models
embed-tool server start --port 8080 --models potion-32M,code-distilled

# Start with authentication
embed-tool server start --auth-required --jwks-url https://auth.example.com/.well-known/jwks.json

# Start in daemon mode
embed-tool server start --daemon --log-file /var/log/embed-tool.log

# Check server status
embed-tool server status

# Stop server
embed-tool server stop

# Restart server
embed-tool server restart
```

#### Model Operations

```bash
# List available models
embed-tool model list

# Download a model
embed-tool model download potion-32M

# Distill a custom model from HuggingFace
embed-tool model distill sentence-transformers/all-MiniLM-L6-v2 my-custom-model --dims 256

# Get model information
embed-tool model info potion-32M

# Remove a model
embed-tool model remove old-model
```

#### Configuration Management

```bash
# Set configuration values
embed-tool config set server.port 8080
embed-tool config set auth.require_auth true
embed-tool config set models.default potion-32M

# Get configuration
embed-tool config get
embed-tool config get server.port

# Reset configuration to defaults
embed-tool config reset
```

#### Embedding Generation

```bash
# Generate single embedding
embed-tool embed "Hello, world!" --model potion-32M

# Generate with specific endpoint
embed-tool embed "text" --endpoint http://localhost:8080

# Batch process from file
embed-tool batch input.jsonl --output results.jsonl --model code-distilled

# Batch with custom format
embed-tool batch texts.txt --output embeddings.json --format json
```

### HTTP API Reference

#### Generate Embeddings

```bash
# Single text embedding
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{
    "input": "Hello, world!",
    "model": "potion-32M"
  }'

# Multiple text embeddings
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "input": ["Text 1", "Text 2", "Text 3"],
    "model": "potion-32M",
    "encoding_format": "float"
  }'
```

#### List Models

```bash
# Get available models
curl -X GET http://localhost:8080/v1/models \
  -H "Authorization: Bearer YOUR_TOKEN"
```

#### Health Check

```bash
# Check server health
curl -X GET http://localhost:8080/health
```

### Configuration File Reference

#### TOML Configuration

```toml
[server]
port = 8080
host = "0.0.0.0"
workers = 4

[auth]
require_auth = true
jwks_url = "https://auth.example.com/.well-known/jwks.json"
audience = "embedding-api"

[models]
default = "potion-32M"
available = ["potion-8M", "potion-32M", "code-distilled"]
path = "/opt/models"

[rate_limit]
rps = 100
burst = 200
enabled = true

[logging]
level = "info"
format = "json"
```

### Environment Variables

```bash
# Server configuration
export EMBED_TOOL_SERVER_PORT=8080
export EMBED_TOOL_SERVER_HOST="0.0.0.0"

# Authentication
export EMBED_TOOL_AUTH_REQUIRE=true
export EMBED_TOOL_AUTH_JWKS_URL="https://auth.example.com/.well-known/jwks.json"

# Models
export EMBED_TOOL_MODELS_DEFAULT="potion-32M"
export EMBED_TOOL_MODELS_PATH="/custom/models"

# Rate limiting
export EMBED_TOOL_RATE_LIMIT_RPS=100
export EMBED_TOOL_RATE_LIMIT_BURST=200
```

### Common Patterns

#### Production Deployment

```bash
# Start server with production settings
embed-tool server start \
  --port 8080 \
  --auth-required \
  --jwks-url https://auth.example.com/.well-known/jwks.json \
  --rate-limit-rps 100 \
  --daemon \
  --log-file /var/log/embed-tool.log

# Monitor server health
embed-tool server status

# Configure log rotation and monitoring
# (Use systemd, supervisor, or similar for production)
```

#### Development Setup

```bash
# Start server without authentication for development
embed-tool server start --port 8080 --auth-disabled

# Test with sample text
embed-tool embed "This is a test sentence" --model potion-32M

# Process batch of test data
echo '{"text": "Sample 1"}\n{"text": "Sample 2"}' | embed-tool batch --input - --output results.jsonl
```

#### Model Comparison

```bash
# Compare models on same text
embed-tool embed "machine learning" --model potion-8M > embedding_8m.json
embed-tool embed "machine learning" --model potion-32M > embedding_32m.json

# Compare performance
time embed-tool embed "performance test" --model potion-8M
time embed-tool embed "performance test" --model potion-32M
```

The Static Embedding Tool server is ready to help you generate high-quality embeddings for your text data!