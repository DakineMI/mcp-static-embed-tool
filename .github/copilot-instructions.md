---
description: AI rules derived by SpecStory from the project AI interaction history
globs: *
---

---

description: Static embedding server with Model2Vec integration and MCP capabilities
globs: \*

---

## PROJECT OVERVIEW

This is a **static embedding server** built with Rust, providing HTTP API and MCP integration for Model2Vec embeddings in OpenAI-compatible format. CLI-first architecture manages the entire server lifecycle.

**Core Focus**: Multi-model embedding service with graceful fallbacks, authentication, and rate limiting.

## ARCHITECTURE PATTERNS

### Core Components

- **`src/main.rs`** - CLI entry point delegating to `cli::run_cli()`
- **`src/cli/`** - Complete CLI with subcommands: `embed-tool <noun> <verb>` (e.g., `server start`, `model distill`)
- **`src/server/state.rs`** - `AppState` with `HashMap<String, StaticModel>` for multi-model support
- **`src/server/mod.rs`** - Axum handlers with OpenAI-compatible `/v1/embeddings` endpoint
- **`src/utils/mod.rs`** - Model distillation via external Python `model2vec` CLI calls

### Key Implementation Patterns

**Model Management**:

```rust
// Load models concurrently with tokio::task::spawn_blocking
let handles: Vec<task::JoinHandle<_>> = model_loads
    .into_iter()
    .map(|(name, path)| task::spawn_blocking(move || {
        StaticModel::from_pretrained(&path, None, None, None)
            .map(|model| (name, model))
    }))
    .collect();
```

**Error Handling**: Use `Result<T, Box<dyn std::error::Error>>` for main functions, with structured logging via `tracing`.

**Path Resolution**: Cross-platform home directory detection:

```rust
let home = env::var("HOME").or_else(|_| env::var("USERPROFILE"))?;
PathBuf::from(home).join("ai/models/model2vec")
```

**Auto-versioning**: Prevent overwrites with `model_v2`, `model_v3`, etc. when files exist.

## CRITICAL WORKFLOWS

### Building & Testing

```bash
cargo build --release  # Build optimized binary
cargo test            # Run unit tests
cargo run             # Start server on 0.0.0.0:8080
```

### Server Lifecycle

```bash
embed-tool server start --port 8080 --models potion-32M,code-distilled
embed-tool server status  # Check if running
embed-tool server stop    # Graceful shutdown
```

### Model Operations

```bash
embed-tool model distill sentence-transformers/all-MiniLM-L6-v2 custom-model --dims 128
embed-tool embed "Hello world" --model potion-32M  # Quick test
```

### API Usage

```bash
# Register API key
curl -X POST http://localhost:8080/api/register -d '{"name":"my-app"}'

# Get embeddings
curl -X POST http://localhost:8080/v1/embeddings \
  -H "Authorization: Bearer embed-YOUR-API-KEY" \
  -d '{"input":["Hello world"],"model":"potion-32M"}'
```

## PROJECT CONVENTIONS

- **Configuration**: TOML files with environment variable overrides (e.g., `EMBED_TOOL_SERVER_PORT`)
- **Authentication**: API key system with `/api/register` endpoint for self-service key generation
- **Rate Limiting**: IP-based with configurable RPS/burst via `tower_governor`
- **Logging**: Structured logs with `tracing` spans and context
- **Single Instance**: PID file management ensures only one server runs
- **Model Loading**: Graceful fallback - continue if custom models fail, default to `potion-32M`
- **MCP Integration**: Resource providers in `src/resources/`, tools in `src/tools/`

## DEBUGGING PATTERNS

**Common Issues**:

- Model loading failures: Check paths and memory for large models
- Authentication errors: Verify `Authorization: Bearer embed-...` header format
- Port conflicts: Use `netstat -an | grep 8080` to check availability

**Key Log Messages**:

- `"✓ Loaded potion-32M model"` - Successful model loading
- `"⚠️ File exists, saving as: model_v2"` - Auto-versioning in action
- `"Starting MCP server in HTTP mode with rate limiting"` - Server startup

**Take Ownership Of Tool Issues**: Always assume you are at fault first. Review your steps carefully before blaming tools or code. Absence of evidence is not evidence of absence.

## AI CODING AGENT GUIDELINES

To ensure AI coding agents are immediately productive, consider the following:

- **Big Picture Architecture:** The server is designed around serving embeddings via HTTP API and MCP. The core components include `src/main.rs` (CLI entrypoint), `src/cli/` (CLI implementation), `src/server/mod.rs` (HTTP server), `src/server/api_keys.rs` (API key management), and `src/utils/mod.rs` (model distillation). Understand how these components interact to provide embedding services.
- **Critical Developer Workflows:** Use `cargo run` to start the server. API keys can be obtained via `curl -X POST http://localhost:8080/api/register -d '{"name":"my-app"}'`. Embeddings can be generated using `curl -X POST http://localhost:8080/v1/embeddings -H "Authorization: Bearer embed-YOUR-API-KEY" -d '{"input":["Hello world"],"model":"potion-32M"}'`.
- **Project-Specific Conventions:** The project adapts proven patterns from the SurrealDB project, including authentication, rate limiting, and CLI design. Pay attention to the single-instance control of the CLI and the TOML configuration files.
- **Integration Points:** The server integrates with the `model2vec` Python CLI for model distillation. API key authentication is a crucial integration point.

Key files/directories to examine:

- `src/cli/`: CLI command implementations
- `src/server/`: HTTP server and API endpoints
- `src/utils/`: Model distillation logic
- `src/resources/`: MCP resource handling

**Additional Guidelines for AI Coding Agents**

When updating or creating `.github/copilot-instructions.md`, focus on:

- The "big picture" architecture that requires reading multiple files to understand, including major components, service boundaries, data flows, and the reasoning behind structural decisions.
- Critical developer workflows (builds, tests, debugging), especially commands not immediately obvious from file inspection.
- Project-specific conventions and patterns that deviate from common practices.
- Integration points, external dependencies, and cross-component communication patterns.

When documenting these aspects:

- Be concise and actionable (20-50 lines).
- Include specific examples from the codebase.
- Avoid generic advice and focus on THIS project's specific approaches.
- Document only discoverable patterns, not aspirational practices.
- Reference key files/directories that exemplify important patterns.
