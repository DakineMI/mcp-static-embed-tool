# Static Embedding Server Server

You are connected to a Static Embedding Server MCP server that provides tools for generating text embeddings using Model2Vec models. The server offers both HTTP API and MCP (Model Context Protocol) integration for AI assistants and development tools.

Use the CLI tools to manage the server, configure models, and generate embeddings. Full instructions and documentation are available as resources and through the CLI help system.

**IMPORTANT**: This is an embedding server, not a database. It specializes in converting text into numerical vector representations using Model2Vec models. Always refer to the tool instructions, the CLI help system, or the documentation for accurate usage patterns.

## Server Overview

The Static Embedding Server provides:

- **OpenAI-compatible HTTP API** at `/v1/embeddings`
- **Multiple Model2Vec models** (potion-8M, potion-32M, code-distilled)
- **CLI-first management** with comprehensive server lifecycle control
- **Authentication** via JWT/JWE bearer tokens with JWKS validation
- **Rate limiting** with configurable IP-based restrictions
- **MCP integration** for AI assistant connectivity
- **Model distillation** capabilities for custom embeddings

## Quick Start

```bash
# Start the embedding server
embed-tool server start --port 8080 --models potion-32M,code-distilled

# Check server status
embed-tool server status

# Generate embeddings via CLI
embed-tool embed "Hello, world!" --model potion-32M

# Generate embeddings via HTTP API
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"input": ["Hello, world!"], "model": "potion-32M"}'
```

## Available Endpoints

### `/v1/embeddings` (POST)
Generate embeddings for input text in OpenAI-compatible format.

### `/v1/models` (GET)  
List available embedding models and their specifications.

### `/health` (GET)
Server health check with model status and uptime information.

### `/.well-known/oauth-protected-resource` (GET)
OAuth discovery endpoint for authentication configuration.

## Model Management

The server supports multiple embedding models:

- **potion-8M**: Fast, lightweight model (256 dimensions)
- **potion-32M**: Balanced performance and quality (256 dimensions) 
- **code-distilled**: Specialized for code embeddings (if available)
- **Custom models**: User-distilled models via `embed-tool model distill`

## Configuration

Server behavior is controlled through:

- **CLI arguments**: `--port`, `--models`, `--auth-required`, etc.
- **Configuration files**: TOML format with hierarchical settings
- **Environment variables**: Override any configuration value
- **Runtime commands**: Live configuration via `embed-tool config`

## Authentication & Security

- **Bearer token validation** with JWT/JWE support
- **JWKS integration** for public key fetching
- **Audience validation** for token scope control
- **IP-based rate limiting** with burst protection
- **Configurable security levels** from development to production

This server is designed for high-performance embedding generation with enterprise-grade security and management capabilities.
