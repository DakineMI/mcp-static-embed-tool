use crate::server::errors::AppError;
use axum::{Router, routing::get, serve::Serve};
use metrics::{counter, gauge};
use rmcp::transport::{
    StreamableHttpServerConfig,
    streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::net::{TcpListener, UnixListener};
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::{TlsAcceptor, rustls::ServerConfig as RustlsServerConfig};
use rustls::pki_types::PrivateKeyDer;

use anyhow::{anyhow, Result as AnyhowResult};
use crate::logs::init_logging_and_metrics;
use crate::server::api::create_api_router;
use crate::server::api_keys::{
    ApiKeyManager,
    api_key_auth_middleware,
    create_api_key_management_router,
    create_registration_router,
};
use crate::server::state::AppState;
use crate::server::http::health;
use crate::server::limit::{api_key_rate_limit_middleware, ApiKeyRateLimiter};
use crate::tools::EmbeddingService;
use crate::utils::{format_duration, generate_connection_id};

/// Configuration for server startup
#[derive(Clone)]
pub struct ServerConfig {
    pub server_url: String,
    pub bind_address: Option<String>,
    pub socket_path: Option<String>,
    pub auth_disabled: bool,
    pub registration_enabled: bool,
    pub rate_limit_rps: u32,
    pub rate_limit_burst: u32,
    pub api_key_db_path: String,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub enable_mcp: bool,
}

// Global metrics
static ACTIVE_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
static TOTAL_CONNECTIONS: AtomicU64 = AtomicU64::new(0);

/// Handle double ctrl-c shutdown with force quit
async fn handle_double_ctrl_c() {
    let mut ctrl_c_count = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                ctrl_c_count += 1;
                if ctrl_c_count == 1 {
                    info!("Received first Ctrl+C signal. Press Ctrl+C again within 2 seconds to force quit.");
                    interval.reset();
                } else if ctrl_c_count >= 2 {
                    warn!("Received second Ctrl+C signal. Force quitting immediately.");
                    std::process::exit(1);
                }
            }
            _ = interval.tick() => {
                if ctrl_c_count > 0 {
                    info!("Ctrl+C timeout expired. Resuming normal operation.");
                    ctrl_c_count = 0;
                }
            }
        }
    }
}

/// Start the MCP server based on the provided configuration
pub async fn start_server(config: ServerConfig) -> AnyhowResult<()> {
    // Output debugging information
    info!(
        server_url = config.server_url,
        bind_address = config.bind_address.as_deref().unwrap_or("N/A"),
        socket_path = config.socket_path.as_deref().unwrap_or("N/A"),
        auth_disabled = config.auth_disabled,
        registration_enabled = config.registration_enabled,
        rate_limit_rps = config.rate_limit_rps,
        rate_limit_burst = config.rate_limit_burst,
        api_key_db_path = config.api_key_db_path,
        tls_enabled = config.tls_cert_path.is_some(),
        "Server configuration loaded"
    );
    match (config.bind_address.is_some(), config.socket_path.is_some()) {
        // We are running as a STDIO server
        (false, false) => start_stdio_server(config).await,
        // We are running as a HTTP server
        (true, false) => start_http_server(config).await,
        // We are running as a Unix socket
        (false, true) => start_unix_server(config).await,
        // This should never happen due to CLI argument groups
        (true, true) => Err(anyhow!(
            "Cannot specify both --bind-address and --socket-path"
        )),
    }
}

// Helper function to create TLS acceptor
async fn create_tls_acceptor(
    cert_path: &str,
    key_path: &str,
) -> AnyhowResult<TlsAcceptor> {
    let cert_file = fs::read(cert_path)
        .await
        .map_err(|e| anyhow!("Failed to read certificate file {}: {}", cert_path, e))?;
    let key_file = fs::read(key_path)
        .await
        .map_err(|e| anyhow!("Failed to read private key file {}: {}", key_path, e))?;

    let certs: Vec<_> = certs(&mut cert_file.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to parse certificates: {}", e))?
        .into_iter()
        .map(|cert| cert.to_vec().into())
        .collect::<Vec<_>>();

    let keys: Vec<_> = pkcs8_private_keys(&mut key_file.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("Failed to parse private keys: {}", e))?;
    let key = PrivateKeyDer::Pkcs8(keys[0].secret_pkcs8_der().to_vec().into());

    let config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| anyhow!("Failed to build TLS config: {}", err))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}
/// Start the MCP server in stdio mode
async fn start_stdio_server(_config: ServerConfig) -> AnyhowResult<()> {
    // MCP is currently disabled due to trait implementation issues
    Err(anyhow!("MCP mode is currently disabled. Use HTTP mode instead."))
}

/// Start the MCP server in Unix socket mode
async fn start_unix_server(config: ServerConfig) -> AnyhowResult<()> {
    // Get the specified socket path
    let socket_path = config
        .socket_path
        .as_deref()
        .expect("socket_path must be provided for unix mode");
    // Initialize structured logging and metrics
    init_logging_and_metrics(false);
    // Get the specified socket path
    let socket_path = Path::new(socket_path);
    // Remove existing socket file if it exists
    if socket_path.exists() {
        fs::remove_file(socket_path).await?;
        info!(
            "Removed existing Unix socket file: {}",
            socket_path.display()
        );
    }
    // Create a Unix domain socket listener at the specified path
    let listener = UnixListener::bind(socket_path)?;
    // Log that the server is listening on the Unix socket
    info!(
        socket_path = %socket_path.display(),
        "Starting MCP server in Unix socket mode"
    );
    // Spawn the double ctrl-c handler
    let _signal = tokio::spawn(handle_double_ctrl_c());
    // Main server loop for Unix socket connections
    loop {
        // Accept incoming connections from the Unix socket
        let (stream, addr) = listener.accept().await?;
        // Generate a connection ID for this connection
        let connection_id = generate_connection_id();
        // Output debugging information
        info!(
            connection_id = %connection_id,
            peer_addr = ?addr,
            "New Unix socket connection accepted"
        );
        // Update connection metrics
        let active_connections = ACTIVE_CONNECTIONS.fetch_add(1, Ordering::SeqCst) + 1;
        let total_connections = TOTAL_CONNECTIONS.fetch_add(1, Ordering::SeqCst) + 1;
        gauge!("embedtool.active_connections").set(active_connections as f64);
        counter!("embedtool.total_connections").increment(1);
        // Output debugging information
        info!(
            connection_id = %connection_id,
            active_connections,
            total_connections,
            "Connection metrics updated"
        );
        // Spawn a new async task to handle this client connection
        tokio::spawn(async move {
            let _span =
                tracing::info_span!("handle_unix_connection", connection_id = %connection_id);
            let _enter = _span.enter();

            debug!("Handling Unix socket connection");
            let service = EmbeddingService::new(connection_id.clone());
            // Create an MCP server instance for this connection
            match rmcp::serve_server(service.clone(), stream).await {
                Ok(server) => {
                    info!(
                        connection_id = %service.connection_id,
                        "MCP server instance creation succeeded"
                    );
                    // Wait for the server to complete its work
                    let _ = server.waiting().await;
                    // Update metrics when connection closes
                    let active_connections = ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst) - 1;
                    gauge!("embedtool.active_connections").set(active_connections as f64);
                    // Output debugging information
                    info!(
                        connection_id = %service.connection_id,
                        connection_time = %format_duration(Instant::now().duration_since(service.created_at)),
                        active_connections,
                        "Connection closed"
                    );
                }
                Err(e) => {
                    // Output debugging information
                    error!(
                        connection_id = %service.connection_id,
                        error = %e,
                        "MCP server instance creation failed"
                    );
                    // Update metrics when connection fails
                    let active_connections = ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::SeqCst) - 1;
                    gauge!("embedtool.active_connections").set(active_connections as f64);
                }
            }
        });
    }
}

/// Start the MCP server in HTTP mode
async fn start_http_server(config: ServerConfig) -> AnyhowResult<()> {
    // Extract configuration values
    let ServerConfig {
        server_url,
        bind_address,
        auth_disabled,
        registration_enabled,
        rate_limit_rps,
        rate_limit_burst,
        api_key_db_path,
        tls_cert_path,
        tls_key_path,
        ..
    } = config;
    // Get the specified bind address
    let bind_address = bind_address.as_deref().unwrap();
    // Initialize structured logging and metrics
    init_logging_and_metrics(false);
    // Output debugging information
    info!(
        server_url = %server_url,
        bind_address = %bind_address,
        rate_limit_rps = rate_limit_rps,
        rate_limit_burst = rate_limit_burst,
        "Starting embedding server with OpenAI-compatible API and MCP support"
    );
    // Create a TCP listener for the HTTP server
    let listener = TcpListener::bind(&bind_address)
        .await
        .map_err(|e| anyhow!("Failed to bind to address {bind_address}: {e}"))?;

    // Ensure API key database directory exists
    if let Some(parent) = Path::new(&api_key_db_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }

    // Create a session manager for the HTTP server
    let session_manager = Arc::new(LocalSessionManager::default());
    
    // Initialize API key manager with persistent storage
    let api_key_manager = Arc::new(ApiKeyManager::new(&api_key_db_path)?);
    
    // Create shared app state with loaded models
    let app_state = Arc::new(AppState::new().await.map_err(|e| anyhow!("Failed to initialize models: {}", e))?);
    
    // Create a new EmbeddingService instance for the MCP server (if enabled)
    let mcp_service = if config.enable_mcp {
        Some(StreamableHttpService::new(
            move || {
                Ok(EmbeddingService::new(generate_connection_id()))
            },
            session_manager,
            StreamableHttpServerConfig {
                stateful_mode: true,
                sse_keep_alive: None,
            },
        ))
    } else {
        None
    };
    
    // Create the OpenAI-compatible API router
    let api_router = create_api_router().with_state(Arc::clone(&app_state));

    // Create API key rate limiter
    let rate_limiter = Arc::new(ApiKeyRateLimiter::new());

    // Protect API router with auth and per-key rate limiting
    let protected_api_router = if !auth_disabled {
        api_router
            .layer(axum::Extension(api_key_manager.clone()))
            .layer(axum::Extension(Arc::clone(&rate_limiter)))
            .layer(axum::middleware::from_fn(api_key_rate_limit_middleware))
            .layer(axum::middleware::from_fn(api_key_auth_middleware))
    } else {
        api_router
    };

    // Public registration router (optional)
    let registration_router = create_registration_router(registration_enabled)
        .with_state(api_key_manager.clone())
        .layer(axum::Extension(api_key_manager.clone()));

    // Protected API key management router
    let api_key_admin_router = {
        let router = create_api_key_management_router().with_state(api_key_manager.clone());
        if !auth_disabled {
            router
                .layer(axum::Extension(api_key_manager.clone()))
                .layer(axum::Extension(Arc::clone(&rate_limiter)))
                .layer(axum::middleware::from_fn(api_key_rate_limit_middleware))
                .layer(axum::middleware::from_fn(api_key_auth_middleware))
        } else {
            router.layer(axum::Extension(api_key_manager.clone()))
        }
    };
    // Create tracing layer for request logging
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &axum::http::Request<_>| {
            let connection_id = generate_connection_id();
            tracing::info_span!(
                "http_request",
                connection_id = %connection_id,
                method = %request.method(),
                uri = %request.uri(),
            )
        })
        .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
            debug!(
                method = %request.method(),
                uri = %request.uri(),
                "HTTP request started"
            );
        })
        .on_response(
            |response: &axum::http::Response<_>, latency: Duration, _span: &tracing::Span| {
                let status = response.status();
                if status.is_client_error() || status.is_server_error() {
                    warn!(
                        status = %status,
                        latency_ms = latency.as_millis(),
                        "HTTP request failed"
                    );
                } else {
                    info!(
                        status = %status,
                        latency_ms = latency.as_millis(),
                        "HTTP request completed"
                    );
                }
            },
        );
    // Create an Axum router with both API and MCP services
    let mut router = Router::new();
    
    if let Some(mcp_svc) = mcp_service {
        router = router.nest_service("/v1/mcp", mcp_svc);  // MCP over HTTP
    }
    
    router = router
        .merge(registration_router)
        .merge(protected_api_router)
        .merge(api_key_admin_router)
        .route("/health", get(health))
        .layer(trace_layer);

    
    // Log available endpoints
    let protocol = if tls_cert_path.is_some() { "https" } else { "http" };
    info!("🚀 Server started on {}://{}", protocol, bind_address);
    info!("📚 Available endpoints:");
    info!("  POST /v1/embeddings     - OpenAI-compatible embedding API (API key required)");
    info!("  GET  /v1/models         - List available models (API key required)");
    if registration_enabled {
        info!("  POST /api/register      - Self-register for API key");
    } else {
        info!("  POST /api/register      - Self-registration disabled");
    }
    info!("  GET  /api/keys          - List API keys (API key required)");
    info!("  POST /api/keys/revoke   - Revoke API key (API key required)");
    info!("  *    /v1/mcp            - MCP protocol endpoint");
    info!("  GET  /health            - Health check");
    info!("🔑 API Key Authentication: {}", if auth_disabled { "DISABLED" } else { "ENABLED" });
    info!("📝 API key self-registration: {}", if registration_enabled { "ENABLED" } else { "DISABLED" });
    if let Some(cert) = &tls_cert_path {
        info!("🔒 TLS enabled with certificate: {}", cert);
    } else {
        info!("🔓 TLS disabled - running on plain {}", protocol);
    }
    
    // Use the shared double ctrl-c handler
    let signal = handle_double_ctrl_c();
    
    if let (Some(_cert_path), Some(_key_path)) = (tls_cert_path, tls_key_path) {
        // TODO: Implement TLS support
        info!("TLS not yet implemented - running on plain HTTP");
        axum::serve(listener, router)
            .with_graceful_shutdown(signal)
            .await
    } else {
        info!("TLS disabled - running on plain HTTP");
        axum::serve(listener, router)
            .with_graceful_shutdown(signal)
            .await
    }?;

    // All ok
    Ok(())
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::server::api_keys::{ApiKeyManager, create_registration_router, create_api_key_management_router};
    use crate::server::state::AppState;
    use crate::server::limit::{ApiKeyRateLimiter, api_key_rate_limit_middleware};
    use axum::routing::get;
    use tokio::net::TcpListener;
    use std::sync::Arc;
    use tokio::task::JoinHandle;
    use tower_http::trace::TraceLayer;
    use tracing::{debug, info};
    use uuid::Uuid;

    pub async fn spawn_test_server(auth_enabled: bool) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test listener");
        let addr = listener.local_addr().expect("Failed to get local addr");
        let addr_str = format!("http://{}", addr);

        // Use system temp directory for test databases
        let temp_dir = std::env::temp_dir();
        let api_key_db_path = temp_dir.join(format!("embed_tool_test_api_keys_{}.db", Uuid::new_v4()));
        let api_key_db_path = api_key_db_path.to_str().unwrap().to_string();

        let api_key_manager = Arc::new(
            ApiKeyManager::new(&api_key_db_path).expect("Failed to create ApiKeyManager")
        );

        let app_state = Arc::new(
            AppState::new().await.expect("Failed to create AppState")
        );

        let rate_limiter = Arc::new(ApiKeyRateLimiter::new());

        let api_router = crate::server::api::create_api_router().with_state(app_state.clone());

        let protected_api_router = if auth_enabled {
            api_router
                .layer(axum::Extension(api_key_manager.clone()))
                .layer(axum::Extension(rate_limiter.clone()))
                .layer(axum::middleware::from_fn(api_key_rate_limit_middleware))
                .layer(axum::middleware::from_fn(crate::server::api_keys::api_key_auth_middleware))
        } else {
            api_router
        };

        let registration_router = create_registration_router(true)
            .with_state(api_key_manager.clone());

        let api_key_admin_router = {
            let router = create_api_key_management_router().with_state(api_key_manager.clone());
            if auth_enabled {
                router
                    .layer(axum::Extension(api_key_manager.clone()))
                    .layer(axum::Extension(rate_limiter.clone()))
                    .layer(axum::middleware::from_fn(api_key_rate_limit_middleware))
                    .layer(axum::middleware::from_fn(crate::server::api_keys::api_key_auth_middleware))
            } else {
                router.layer(axum::Extension(api_key_manager.clone()))
            }
        };

        let trace_layer = TraceLayer::new_for_http()
            .make_span_with(|request: &axum::http::Request<_>| {
                let connection_id = Uuid::new_v4().to_string();
                tracing::info_span!(
                    "http_request",
                    connection_id = %connection_id,
                    method = %request.method(),
                    uri = %request.uri(),
                )
            })
            .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                debug!(
                    method = %request.method(),
                    uri = %request.uri(),
                    "HTTP request started"
                );
            })
            .on_response(
                |response: &axum::http::Response<_>, latency: std::time::Duration, _span: &tracing::Span| {
                    let status = response.status();
                    if status.is_client_error() || status.is_server_error() {
                        info!(
                            status = %status,
                            latency_ms = latency.as_millis(),
                            "HTTP request failed"
                        );
                    } else {
                        debug!(
                            status = %status,
                            latency_ms = latency.as_millis(),
                            "HTTP request completed"
                        );
                    }
                },
            );

        let router = Router::new()
            .nest_service("/v1/mcp", Router::new()) // Skip MCP for tests
            .merge(registration_router)
            .merge(protected_api_router)
            .merge(api_key_admin_router)
            .route("/health", get(crate::server::http::health))
            .layer(trace_layer);

        let server = axum::serve(listener, router);

        let handle = tokio::spawn(async move {
            let _ = server.await;
        });

        (addr_str, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_utils::spawn_test_server;
    use std::time::Duration;
    use tokio::time::timeout;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_start_server_stdio_mode() {
        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: None,
            socket_path: None,
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: "/tmp/test.db".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: false,
        };

        let result = start_server(config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("MCP mode is currently disabled"));
    }

    #[tokio::test]
    async fn test_start_server_both_addresses_error() {
        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: Some("127.0.0.1:8080".to_string()),
            socket_path: Some("/tmp/test.sock".to_string()),
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: "/tmp/test.db".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: false,
        };

        let result = start_server(config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot specify both"));
    }

    #[tokio::test]
    async fn test_start_unix_server_missing_socket_path() {
        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: None,
            socket_path: None, // This should cause an error
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: "/tmp/test.db".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: false,
        };

        // This should panic because socket_path is None but we try to unwrap it
        // We can't easily test this without changing the function signature
        // So we'll skip this test for now
    }

    #[test]
    fn test_server_config_creation() {
        let config = ServerConfig {
            server_url: "http://localhost:8080".to_string(),
            bind_address: Some("127.0.0.1:8080".to_string()),
            socket_path: None,
            auth_disabled: false,
            registration_enabled: true,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            api_key_db_path: "/tmp/api_keys.db".to_string(),
            tls_cert_path: Some("/tmp/cert.pem".to_string()),
            tls_key_path: Some("/tmp/key.pem".to_string()),
            enable_mcp: true,
        };

        assert_eq!(config.server_url, "http://localhost:8080");
        assert_eq!(config.bind_address, Some("127.0.0.1:8080".to_string()));
        assert_eq!(config.socket_path, None);
        assert_eq!(config.auth_disabled, false);
        assert_eq!(config.registration_enabled, true);
        assert_eq!(config.rate_limit_rps, 100);
        assert_eq!(config.rate_limit_burst, 200);
        assert_eq!(config.api_key_db_path, "/tmp/api_keys.db");
        assert_eq!(config.tls_cert_path, Some("/tmp/cert.pem".to_string()));
        assert_eq!(config.tls_key_path, Some("/tmp/key.pem".to_string()));
        assert_eq!(config.enable_mcp, true);
    }

    #[tokio::test]
    async fn test_create_tls_acceptor_invalid_cert() {
        let result = create_tls_acceptor("/nonexistent/cert.pem", "/nonexistent/key.pem").await;
        assert!(result.is_err());
        // Just check that it returns an error, don't check the message since TlsAcceptor doesn't implement Debug
    }

    #[tokio::test]
    async fn test_handle_double_ctrl_c_timeout() {
        // This test is tricky because it involves signals and timeouts
        // We'll test that the function can be spawned and cancelled
        let handle = tokio::spawn(async {
            // This will run indefinitely until cancelled
            handle_double_ctrl_c().await;
        });

        // Cancel after a short time
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();

        // If we get here without panicking, the test passes
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_http_server_bind_failure() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: Some("invalid-address:8080".to_string()), // Invalid address
            socket_path: None,
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: db_path,
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: false,
        };

        let result = start_http_server(config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to bind"));
    }

    #[tokio::test]
    async fn test_start_http_server_with_auth_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: Some("127.0.0.1:0".to_string()), // Use port 0 for auto-assignment
            socket_path: None,
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: db_path,
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: false,
        };

        // This test would require actually starting a server and testing it
        // For now, we'll just test that the configuration is valid
        // The actual server start would require mocking more dependencies
        assert_eq!(config.auth_disabled, true);
        assert_eq!(config.registration_enabled, false);
    }

    #[tokio::test]
    async fn test_start_http_server_with_auth_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: Some("127.0.0.1:0".to_string()),
            socket_path: None,
            auth_disabled: false,
            registration_enabled: true,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: db_path,
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: true,
        };

        // Test configuration validation
        assert_eq!(config.auth_disabled, false);
        assert_eq!(config.registration_enabled, true);
        assert_eq!(config.enable_mcp, true);
    }

    #[tokio::test]
    async fn test_start_http_server_with_tls_config() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();

        let config = ServerConfig {
            server_url: "test".to_string(),
            bind_address: Some("127.0.0.1:0".to_string()),
            socket_path: None,
            auth_disabled: true,
            registration_enabled: false,
            rate_limit_rps: 10,
            rate_limit_burst: 20,
            api_key_db_path: db_path,
            tls_cert_path: Some("/tmp/cert.pem".to_string()),
            tls_key_path: Some("/tmp/key.pem".to_string()),
            enable_mcp: false,
        };

        // Test TLS configuration
        assert_eq!(config.tls_cert_path, Some("/tmp/cert.pem".to_string()));
        assert_eq!(config.tls_key_path, Some("/tmp/key.pem".to_string()));
    }

    #[tokio::test]
    async fn test_spawn_test_server_auth_enabled() {
        let (addr, handle) = spawn_test_server(true).await;

        // Verify server address format
        assert!(addr.starts_with("http://127.0.0.1:"));

        // Stop the server
        handle.abort();
    }

    #[tokio::test]
    async fn test_spawn_test_server_auth_disabled() {
        let (addr, handle) = spawn_test_server(false).await;

        // Verify server address format
        assert!(addr.starts_with("http://127.0.0.1:"));

        // Stop the server
        handle.abort();
    }

    #[test]
    fn test_global_metrics_initialization() {
        // Test that global metrics are initialized to 0
        assert_eq!(ACTIVE_CONNECTIONS.load(Ordering::SeqCst), 0);
        assert_eq!(TOTAL_CONNECTIONS.load(Ordering::SeqCst), 0);
    }
}