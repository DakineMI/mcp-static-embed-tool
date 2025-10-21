---
description: AI rules derived by SpecStory from the project AI interaction history
globs: *
---

---

## description: AI rules derived by SpecStory from the project AI interaction history

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

**Error Handling**: Use `Result<T, Box<dyn std::error::Error>>` for main functions, with structured logging via `tracing`. To handle errors returned by handler functions (`anyhow::Result<()>`) in `run_cli` (which expects `Result<(), Box<dyn std::error::Error>>`), convert the error types using `.map_err(Into::into)` in each match arm. When encountering the error: `the method anyhow_kind exists for reference &Box<dyn StdError>, but its trait bounds were not satisfied`, wrap the error in a descriptive message using `anyhow::anyhow!` with formatting. When using the `?` operator on a `Result` where the error type is a `&str`, the `?` operator attempts to convert the error into an `anyhow::Error` using `map_err(|e| anyhow::anyhow!(e))`.
**The error occurs because the `crate::utils::distill` function returns a `Result` with `Box<dyn std::error::Error>`, which lacks the `Send`, `Sync`, and `Sized` traits required for automatic conversion to `anyhow::Error` via `?`. To fix this, wrap the error in `anyhow::anyhow!` using `map_err` before propagating it.** When using the `?` operator on a `Result` where the error type is a `&str`, the `?` operator attempts to convert the error into an `anyhow::Error` using `map_err(|_| anyhow::anyhow!(...))`.

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
- **Test Artifacts**: When running the application or tests, do not generate garbage test artifacts without cleaning them up.

## DEBUGGING PATTERNS

**Common Issues**:

- Model loading failures: Check paths and memory for large models
- Authentication errors: Verify `Authorization: Bearer embed-...` header format
- Port conflicts: Use `netstat -an | grep 8080` to check availability
- **Missing generics for `axum::http::Response`**: Ensure the return type includes the `Body` generic, e.g., `Response<Body>`.
- **Incorrect number of generic arguments for `GovernorLayer`**: Ensure all three generic type arguments are supplied: the key extractor, the middleware, and the state store type (e.g., `GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, KeyedStateStore<String>>`).
- **Mismatched Types with `GovernorLayer`**: If you encounter mismatched types with `GovernorLayer`, where the expected type is `GovernorLayer<_, _, dashmap::DashMap<std::string::String, InMemoryState>>` but the found type is `GovernorLayer<_, _, Body>`, this indicates a type mismatch in the return type. This can be resolved by ensuring the correct state store type (`InMemoryState`) is used when initializing the `GovernorLayer`.
- **Syntax Error: expected an item**: This is often caused by a misplaced duplicate struct definition or incomplete method code. Ensure the code block has a proper structure, remove duplicate structs, correct the type to use `InMemoryState`, and complete any incomplete methods.
- **`embedtool`: Unknown word**: This appears to be a linter error, not a compilation error, and can be ignored, or addressed by adding the word to the linter's dictionary.
- **Expected a type, found a trait**: When encountering this error, consider adding the `dyn` keyword if a trait object is intended (`dyn `).
- **`?` couldn't convert the error: `str: StdError` is not satisfied**: This error arises when using the `?` operator on a `Result` where the error type is a `&str`. The `?` operator attempts to convert the error into an `anyhow::Error`, but `&str` does not implement the `StdError` trait, which is required for this conversion. To fix this, use `map_err` to convert the `&str` into an `anyhow::Error` using `map_err(|e| anyhow::anyhow!(e))`.
- **The error occurs because the `crate::utils::distill` function returns a `Result` with `Box<dyn StdError>`, which lacks the `Send`, `Sync`, and `Sized` traits required for automatic conversion to `anyhow::Error` via `?`. To fix this, wrap the error in `anyhow::anyhow!` using `map_err` before propagating it.**
- **Missing generics for struct `axum::http::Response`**: Ensure the return type includes the `Body` generic, e.g., `Response<Body>`.
- **Incorrect number of generic arguments for `GovernorLayer`**: Ensure all three generic type arguments are supplied: the key extractor, the middleware, and the state store type (e.g., `GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, KeyedStateStore<String>>`).
- **Mismatched Types with `GovernorLayer`**: If you encounter mismatched types with `GovernorLayer`, where the expected type is `GovernorLayer<_, _, dashmap::DashMap<std::string::String, InMemoryState>>` but the found type is `GovernorLayer<_, _, Body>`, this indicates a type mismatch in the return type. This can be resolved by ensuring the correct state store type (`InMemoryState`) is used when initializing the `GovernorLayer`.
- **`E0614: type `std::option::Option<&mut HeaderMap>` cannot be dereferenced`**: In `src/server/limit.rs`, the line `*req.headers_mut() = headers;` is causing a compilation error because `req.headers_mut()` returns an `Option<&mut HeaderMap>`, and you cannot directly dereference and assign to it.
- **Invalid VS Code terminal profile color setting**: The terminal profile color setting in VS Code's `settings.json` only accepts predefined ANSI color names, not arbitrary hex codes. To fix this, replace the invalid hex color with the closest matching ANSI color name.
- **Invalid VS Code terminal profile icon value**: The VS Code terminal profile setting only accepts predefined icon names. To fix this, change it to a valid icon from the allowed list, such as "terminal" (a generic terminal icon).
- **Unresolved import `governor::state::DirectState`**: This is due to changes in the `governor` crate. Update the import to `governor::state::direct::DirectStateStore` or `governor::state::keyed::NotKeyed` as appropriate. In some versions, `DirectState` may have been renamed to `DirectStateStore`, so adjust the import and type references accordingly. **Resolution**: Update the import to `governor::state::direct::DirectStateStore` or `governor::state::NotKeyed` as appropriate, and replace all occurrences of `DirectState` with `DirectStateStore` if necessary.
- **Unresolved import `governor::state::keyed::NotKeyed`**: Update the import to `governor::state::NotKeyed`.
- **Unresolved import `governor::state::direct::DefaultDirectStateStore`**: This is due to changes in the `governor` crate. Remove the unresolved `DefaultDirectStateStore` import and use `DirectStateStore` consistently for direct state stores.
- **Unused imports: `ApiError` and `ErrorDetails`**: Remove the whole `use` item.
- **Unused import `crate::server::errors::AppError`**: Remove the unused import statement.

**Key Log Messages**:

- `"✓ Loaded potion-32M model"` - Successful model loading
- `"⚠️ File exists, saving as: model_v2"` - Auto-versioning in action
- `"Starting MCP server in HTTP mode with rate limiting"` - Server startup

**Take Ownership Of Tool Issues**: Always assume you are at fault first. Review your steps carefully before blaming tools or code. Absence of evidence is not evidence of absence.

**Current Project Status**: The codebase currently compiles cleanly with `cargo build`. The core architecture is sound but incomplete, requiring systematic fixes before testing or deployment. Expect type mismatches, missing trait implementations, and incomplete async handling.

### Key Fixes Applied:

1. **Error Handling Conversion**: Fixed `?` operator errors in `models.rs` by converting `&str` errors to `anyhow::Error` using `map_err(|_| anyhow::anyhow!(...))`.

2. **Import Cleanup**: Removed duplicate imports and added missing `anyhow` macro imports across multiple files (`start.rs`, `state.rs`, `api.rs`, etc.).

3. **Type Annotations**: Updated function return types to use `AnyhowResult<T>` instead of `Result<T>` for consistency.

4. **Iterator Handling**: Fixed certificate parsing by collecting iterators before applying `map_err` in `start.rs`.

5. **TLS Implementation**: Temporarily disabled TLS support in `start.rs` due to API changes in newer versions of `rustls`/`axum` - marked for future implementation.

6. **Rate Limiting**: Commented out rate limiting layer due to type mismatches with the `tower_governor` crate - marked for future resolution.

7. **MCP Tools**: Fixed import issues and temporarily disabled problematic trait implementations in `mod.rs`. Implemented conditional MCP support in the server.

8. **API Key Management**: Fixed deprecated `rand::thread_rng()` usage and type annotation issues in `api_keys.rs`.

9. **Documentation**: Removed unsupported `globs` attribute from `copilot-instructions.md`.

10. **Async TCP Listener**: Fixed `spawn_test_server` to use `tokio::net::TcpListener` instead of `std::net::TcpListener` and added proper `.await` calls.

11. **Axum Server API**: Updated server spawning to use the correct Axum 0.8 API pattern (`axum::serve(listener, router)`).

12. **Example Compilation**: Fixed `api_key_demo.rs` to use correct API methods and struct fields from the updated ApiKeyManager.

13. **Test Database Setup**: Corrected the test manager to use proper database paths instead of incompatible sled Config patterns.

### Current Status:

- ✅ **Compilation**: Code compiles cleanly with `cargo check`
- ✅ **Core Functionality**: HTTP API and CLI work correctly
- ✅ **MCP Support**: Conditionally disabled to resolve conflicts, framework preserved for future re-enablement
- ✅ **Test Suite**: All tests now passing
- ✅ **HTTP API**: Fully functional OpenAI-compatible `/v1/embeddings` endpoint
- ✅ **CLI**: Working server lifecycle management (`server start`, `server stop`, etc.)
- ✅ **Model Management**: Multi-model support with graceful fallbacks
- ✅ **Authentication**: API key system for secure access
- ⚠️ **TLS support** temporarily disabled (needs rustls/axum API update)
- ⚠️ **Rate limiting** temporarily disabled (needs tower_governor compatibility fix)
- ⚠️ **MCP tools** partially disabled (needs rmcp crate API alignment)
- ✅ **Test Suite**: All tests now passing

**Compilation Errors and Fixes**:

- **`HashMap`, `RwLock`, `json`, `ApiKey` defined multiple times**: Remove duplicate import statements, ensuring each is defined only once in the module's namespace.
- **Unresolved import `governor::state::DirectState`**: This is due to changes in the `governor` crate. Update the import to `governor::state::direct::DirectStateStore` or `governor::state::keyed::NotKeyed` as appropriate. In some versions, `DirectState` may have been renamed to `DirectStateStore`, so adjust the import and type references accordingly. **Resolution**: Update the import to `governor::state::direct::DirectStateStore` or `governor::state::NotKeyed` as appropriate, and replace all occurrences of `DirectState` with `DirectStateStore` if necessary.
- **Unresolved import `governor::state::keyed::NotKeyed`**: Update the import to `governor::state::NotKeyed`.
- **Unresolved import `governor::state::direct::DefaultDirectStateStore`**: This is due to changes in the `governor` crate. Remove the unresolved `DefaultDirectStateStore` import and use `DirectStateStore` consistently for direct state stores.
- **Unused imports: `ApiError` and `ErrorDetails`**: Remove the whole `use` item.
- **Unused import `crate::server::errors::AppError`**: Remove the unused import statement.
- **`embedtool`: Unknown word**: This appears to be a linter error, not a compilation error, and can be ignored, or addressed by adding the word to the linter's dictionary.
- **Expected a type, found a trait**: When encountering this error, consider adding the `dyn` keyword if a trait object is intended (`dyn `).
- **`?` couldn't convert the error: `str: StdError` is not satisfied**: This error arises when using the `?` operator on a `Result` where the error type is a `&str`. The `?` operator attempts to convert the error into an `anyhow::Error`, but `&str` does not implement the `StdError` trait, which is required for this conversion. To fix this, use `map_err` to convert the `&str` into an `anyhow::Error` using `map_err(|e| anyhow::anyhow!(e))`.
- **The error occurs because the `crate::utils::distill` function returns a `Result` with `Box<dyn StdError>`, which lacks the `Send`, `Sync`, and `Sized` traits required for automatic conversion to `anyhow::Error` via `?`. To fix this, wrap the error in `anyhow::anyhow!` using `map_err` before propagating it.**
- **Missing generics for struct `axum::http::Response`**: Ensure the return type includes the `Body` generic, e.g., `Response<Body>`.
- **Incorrect number of generic arguments for `GovernorLayer`**: Ensure all three generic type arguments are supplied: the key extractor, the middleware, and the state store type (e.g., `GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, KeyedStateStore<String>>`).
- **Mismatched Types with `GovernorLayer`**: If you encounter mismatched types with `GovernorLayer`, where the expected type is `GovernorLayer<_, _, dashmap::DashMap<std::string::String, InMemoryState>>` but the found type is `GovernorLayer<_, _, Body>`, this indicates a type mismatch in the return type. This can be resolved by ensuring the correct state store type (`InMemoryState`) is used when initializing the `GovernorLayer`.
- **`E0614: type `std::option::Option<&mut HeaderMap>` cannot be dereferenced`**: In `src/server/limit.rs`, the line `*req.headers_mut() = headers;` is causing a compilation error because `req.headers_mut()` returns an `Option<&mut HeaderMap>`, and you cannot directly dereference and assign to it.

## AI CODING AGENT GUIDELINES

To ensure AI coding agents are immediately productive, consider the following:

- **Big Picture Architecture:** The server is designed around serving embeddings via HTTP API and MCP. The core components include `src/main.rs` (CLI entrypoint), `src/cli/` (CLI implementation), `src/server/mod.rs` (HTTP server), `src/server/api_keys.rs` (API key management), and `src/utils/mod.rs` (model distillation). Understand how these components interact to provide embedding services.
- **Critical Developer Workflows:** Use `cargo run` to start the server. API keys can be obtained via `curl -X POST http://localhost:8080/api/register -d '{"name":"my-app"}'`. Embeddings can be generated using `curl -X POST http://localhost:8080/v1/embeddings -H "Authorization: Bearer embed-YOUR-API-KEY" -d '{"input":["Hello world"],"model":"potion-32M}`.
- **Project-Specific Conventions:** The project adapts proven patterns, including authentication, rate limiting, and CLI design. Pay attention to the single-instance control of the CLI and the TOML configuration files. The project also uses auto-versioning for files, graceful model loading fallbacks, and integrates with an external Python CLI.
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
- Avoid generic advice ("write tests", "handle errors") - focus on THIS project's specific approaches.
- Document only discoverable patterns, not aspirational practices.
- Reference key files/directories that exemplify important patterns.
- Use detailed todos to track all work. They should be specific and broken down in reasonable, completable, simple tasks.
- When running the application or tests, do not generate garbage test artifacts without cleaning them up.

**Updates needed for `copilot-instructions.md`:**

1. Remove outdated references (e.g., "transformed from SurrealDB").
2. Update architecture section to reflect current compilation issues and incomplete implementations.
3. Add debugging section including common compilation errors and their fixes.
4. Clarify project status as a work-in-progress.
5. Update workflow examples to reflect current CLI command availability.
6. **Do not create `agent.md`**. The existing `copilot-instructions.md` serves the same purpose.

The AI coding assistant must add a large number of tests, so go through and quickly and efficiently add tests for every method, and once you can not find another test to add, then you can check the percentage. The AI coding assistant does not need to run tarpaulin after every test it adds. You **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin. And every method of every file **MUST** have a test.
The AI coding assistant does not need to run tarpaulin after every test it adds. You **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin. And every method of every file **MUST** have a test.

The AI coding assistant **MUST** wait for commands issued to complete, then read their results **BEFORE** issuing additional commands. The AI coding assistant **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin. And every method of every file **MUST** have a test.
The AI coding assistant **MUST** wait for commands issued to complete, then read their results **BEFORE** issuing additional commands

**All requirements must be met:**

- Every method of every file **MUST** have a test
- **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin.
- The AI coding assistant **MUST** wait for commands issued to complete, then read their results **BEFORE** issuing additional commands
- Code coverage, quality, maintainability, documentation, and readability are all expected to be at or above minimum professional thresholds.

Going forward, use `fd` for file discovery and navigation.

Here are handy fd equivalents you can reuse:

- List all files under src/server
  - fd . src/server -t f
- List all Rust files anywhere under src
  - fd -e rs . src
- Match by full path (when the pattern includes slashes)
  - fd --full-path 'src/server' -t f
- Find test files
  - fd -e rs 'test' src
- List only directories (e.g., modules)
  - fd . src/server -t d

Note: fd’s syntax is fd [pattern] [path]. If your pattern contains a slash, use --full-path, or use a match-all pattern (.) and provide the directory as the path.

It is expected you will write good error free accurate tests the first time without needing to run tests after each one or file.

The AI coding assistant does not need to run tarpaulin after every test it adds. The AI coding assistant **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin. And every method of every file **MUST** have a test.
The AI coding assistant **MUST** wait for commands issued to complete, then read their results **BEFORE** issuing additional commands

When the AI coding assistant generates test cases, it must follow these steps:

1.  Confirm the test case compiles successfully.
2.  Run the test case to confirm successful execution.
3.  After generating tests for each file in the codebase, the AI will estimate whether the overall test coverage has exceeded 95%.
4.  The AI coding assistant **MUST** reasonably estimate that test coverage has exceeded 95% for the project source code before running test or tarpolin. And every method of every file **MUST** have a test.

If the AI coding assistant is unable to automatically test the cases, it will:

- Describe the proposed test case and ask for guidance.
- Provide the rationale behind the test case.
- Explain how to manually run the test and verify the results.

Remember to use fd for file navigation and rg for searching.

When we run the application or tests, we can not generate garbage test artifacts without cleaning it up.

## TECHNICAL DEBT

The following features have been temporarily disabled and need to be addressed:

- **TLS support** - Temporarily disabled due to rustls/axum API update
- **Rate limiting** - Temporarily disabled due to tower_governor compatibility fix
- **MCP tools** - Partially disabled due to rmcp crate API alignment

Prioritize fixing these issues before adding new features or focusing solely on code coverage metrics.

The following features have been temporarily disabled and need to be addressed:

- **TLS support** - Temporarily disabled due to rustls/axum API update
- **Rate limiting** - Temporarily disabled due to tower_governor compatibility fix
- **MCP tools** - Partially disabled due to rmcp crate API alignment

Prioritize fixing these issues before adding new features or focusing solely on code coverage metrics.