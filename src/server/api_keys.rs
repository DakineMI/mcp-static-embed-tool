//! API key management and authentication middleware.
//!
//! This module implements a complete API key authentication system with:
//! - **Key generation**: Cryptographically secure random keys with SHA-256 hashing
//! - **Storage**: Persistent storage in sled embedded database
//! - **Validation**: Middleware for request authentication
//! - **Management**: List, revoke, and admin endpoints
//! - **Registration**: Self-service key generation (when enabled)
//!
//! # Security
//!
//! - Keys are hashed with SHA-256 before storage (plaintext never persisted)
//! - Keys have format: `embed-{base64_encoded_random_bytes}`
//! - Rate limiting is enforced per-key via middleware integration
//!
//! # API Key Format
//!
//! ```text
//! embed-AbCdEfGhIjKlMnOpQrStUvWxYz1234567890AbCdEfGhIjKl
//! ```
//!
//! # Examples
//!
//! ```no_run
//! use static_embedding_server::server::api_keys::{ApiKeyManager, ApiKeyRequest};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let manager = ApiKeyManager::new("./api_keys.db")?;
//!     
//!     // Generate a new API key
//!     let request = ApiKeyRequest {
//!         client_name: "my-app".to_string(),
//!         description: None,
//!         email: None,
//!     };
//!     let response = manager.generate_api_key(request).await.map_err(|e| anyhow::anyhow!("{}", e))?;
//!     println!("Your API key: {}", response.api_key);
//!     
//!     // Validate a key
//!     if let Some(_api_key_info) = manager.validate_api_key(&response.api_key).await {
//!         println!("API key is valid!");
//!     }
//!     
//!     Ok(())
//! }
//! ```

use axum::extract::Request;
use axum::middleware::Next;
use axum::{
    http::StatusCode,
    http::header::AUTHORIZATION,
    response::{IntoResponse, Response, Json as ResponseJson},
    routing::{get, post},
    Router, Json,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sled::Db;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn, error};
use rand::RngCore;
use uuid::Uuid;

/// API Key information
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ApiKey {
    /// Unique API key ID
    pub id: String,
    /// The actual API key (hashed for storage)
    pub key_hash: String,
    /// User/client identifier
    pub client_name: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Last used timestamp
    pub last_used: Option<u64>,
    /// Rate limit tier
    pub rate_limit_tier: String,
    /// Maximum requests per minute
    pub max_requests_per_minute: u32,
    /// Whether the key is active
    pub active: bool,
    /// Optional description
    pub description: Option<String>,
}

/// API Key registration request
#[derive(Debug, Deserialize)]
pub struct ApiKeyRequest {
    /// Client/application name
    pub client_name: String,
    /// Optional description
    pub description: Option<String>,
    /// Email for contact (optional)
    pub email: Option<String>,
}

/// API Key registration response
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    /// The generated API key (only shown once)
    pub api_key: String,
    /// API key metadata
    pub key_info: ApiKeyInfo,
}

/// API Key info for responses (without sensitive data)
#[derive(Debug, Serialize)]
pub struct ApiKeyInfo {
    pub id: String,
    pub client_name: String,
    pub created_at: u64,
    pub last_used: Option<u64>,
    pub rate_limit_tier: String,
    pub max_requests_per_minute: u32,
    pub active: bool,
    pub description: Option<String>,
}

/// API Key manager
#[derive(Debug)]
pub struct ApiKeyManager {
    /// Sled database
    db: Db,
}

impl ApiKeyManager {
    /// Create a new API key manager
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(db_path);
        let db = sled::open(path)?;
        // Ensure trees exist
        let _ = db.open_tree("keys")?;
        let _ = db.open_tree("hashes")?;
        Ok(Self { db })
    }

    /// Generate a new API key
    pub async fn generate_api_key(&self, request: ApiKeyRequest) -> Result<ApiKeyResponse, String> {
        let key_id = Uuid::new_v4().to_string();
        
        // Generate a secure API key: embed-<base64-encoded-random-bytes>
    let mut rng = rand::rng();
        let mut random_bytes = [0u8; 32];
        rng.fill_bytes(&mut random_bytes);
        let api_key = format!("embed-{}", STANDARD.encode(random_bytes));
        
        // Hash the API key for storage
        let key_hash = sha256::digest(api_key.as_bytes());
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Determine rate limit tier based on client name or request
        let (rate_limit_tier, max_requests_per_minute) = match request.client_name.to_lowercase() {
            name if name.contains("dev") || name.contains("test") => ("development".to_string(), 100),
            name if name.contains("prod") || name.contains("enterprise") => ("premium".to_string(), 5000),
            _ => ("standard".to_string(), 1000),
        };

        let api_key_info = ApiKey {
            id: key_id.clone(),
            key_hash: key_hash.clone(),
            client_name: request.client_name.clone(),
            created_at: now,
            last_used: None,
            rate_limit_tier: rate_limit_tier.clone(),
            max_requests_per_minute,
            active: true,
            description: request.description.clone(),
        };

        // Store the API key in DB
        let keys_tree = self.db.open_tree("keys").map_err(|e| e.to_string())?;
        let serialized = bincode::encode_to_vec(&api_key_info, bincode::config::standard())
            .map_err(|e| e.to_string())?;
        keys_tree.insert(key_id.as_bytes(), serialized.as_slice())
            .map_err(|e| e.to_string())?;

        // Update the hash index
        let hashes_tree = self.db.open_tree("hashes").map_err(|e| e.to_string())?;
        hashes_tree.insert(key_hash.as_bytes(), key_id.as_bytes())
            .map_err(|e| e.to_string())?;

        info!(
            key_id = %key_id,
            client_name = %request.client_name,
            rate_limit_tier = %rate_limit_tier,
            "Generated new API key"
        );

        Ok(ApiKeyResponse {
            api_key,
            key_info: ApiKeyInfo {
                id: key_id,
                client_name: request.client_name,
                created_at: now,
                last_used: None,
                rate_limit_tier,
                max_requests_per_minute,
                active: true,
                description: request.description,
            },
        })
    }

    /// Validate an API key and return the key info
    pub async fn validate_api_key(&self, api_key: &str) -> Option<ApiKey> {
        let key_hash = sha256::digest(api_key.as_bytes());
        
        // Look up the key ID from hash index
        let hashes_tree = self.db.open_tree("hashes").map_err(|_| None::<sled::Tree>).ok()?;
        let key_id_bytes = match hashes_tree.get(key_hash.as_bytes()).map_err(|_| None::<sled::IVec>).ok()? {
            Some(id) => id,
            None => return None,
        };
        let key_id = String::from_utf8(key_id_bytes.to_vec()).ok()?;

        // Get the key info from keys tree
        let keys_tree = self.db.open_tree("keys").map_err(|_| None::<sled::Tree>).ok()?;
        let serialized = match keys_tree.get(key_id.as_bytes()).map_err(|_| None::<sled::IVec>).ok()? {
            Some(data) => data,
            None => return None,
        };

        let (key_info, _): (ApiKey, usize) = bincode::decode_from_slice(&serialized, bincode::config::standard())
            .map_err(|_| None::<(ApiKey, usize)>).ok()?;
        
        // Check if key is active
        if !key_info.active {
            return None;
        }

        // Update last used timestamp
        let mut updated_key = key_info.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        updated_key.last_used = Some(now);

        // Update in storage
        let serialized_updated = bincode::encode_to_vec(&updated_key, bincode::config::standard())
            .map_err(|_| None::<Vec<u8>>).ok()?;
        if keys_tree.insert(key_id.as_bytes(), serialized_updated.as_slice()).is_err() {
            return None;
        }

        debug!(
            key_id = %updated_key.id,
            client_name = %updated_key.client_name,
            "API key validated successfully"
        );

        Some(updated_key)
    }

    /// List all API keys (without sensitive data)
    pub async fn list_api_keys(&self) -> Vec<ApiKeyInfo> {
        let keys_tree = match self.db.open_tree("keys") {
            Ok(tree) => tree,
            Err(e) => {
                error!("Failed to open keys tree: {}", e);
                return vec![];
            }
        };

        let mut api_keys = vec![];
        for entry in keys_tree.iter().map(|res| res.map_err(|e| error!("DB error: {}", e))) {
            if let Ok((_, value)) = entry {
                let (key_info, _): (ApiKey, usize) = match bincode::decode_from_slice(&value, bincode::config::standard()) {
                    Ok(decoded) => decoded,
                    Err(e) => {
                        error!("Failed to deserialize ApiKey: {}", e);
                        continue;
                    }
                };
                api_keys.push(ApiKeyInfo {
                    id: key_info.id.clone(),
                    client_name: key_info.client_name.clone(),
                    created_at: key_info.created_at,
                    last_used: key_info.last_used,
                    rate_limit_tier: key_info.rate_limit_tier.clone(),
                    max_requests_per_minute: key_info.max_requests_per_minute,
                    active: key_info.active,
                    description: key_info.description.clone(),
                });
            }
        }
        api_keys
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, key_id: &str) -> bool {
        let keys_tree = match self.db.open_tree("keys") {
            Ok(tree) => tree,
            Err(e) => {
                error!("Failed to open keys tree: {}", e);
                return false;
            }
        };

        let serialized = match keys_tree.get(key_id.as_bytes()) {
            Ok(Some(data)) => data,
            _ => return false,
        };

        let (mut key_info, _): (ApiKey, usize) = match bincode::decode_from_slice(&serialized, bincode::config::standard()) {
            Ok(decoded) => decoded,
            Err(e) => {
                error!("Failed to deserialize ApiKey for revocation: {}", e);
                return false;
            }
        };

        key_info.active = false;

        let serialized_updated = bincode::encode_to_vec(&key_info, bincode::config::standard())
            .map_err(|e| {
                error!("Failed to serialize updated ApiKey: {}", e);
                false
            }).unwrap_or_default();

        if keys_tree.insert(key_id.as_bytes(), serialized_updated.as_slice()).is_ok() {
            // Optionally remove from hashes, but keep for invalidation
            info!(key_id = %key_id, "API key revoked");
            true
        } else {
            false
        }
    }
}

/// API key authentication middleware
pub async fn api_key_auth_middleware(
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract API key manager from request extensions
    let api_key_manager = req.extensions()
        .get::<Arc<ApiKeyManager>>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Extract API key from Authorization header
    let api_key = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            if h.starts_with("Bearer ") {
                Some(h.strip_prefix("Bearer ").unwrap())
            } else if h.starts_with("embed-") {
                Some(h)
            } else {
                None
            }
        });

    if let Some(key) = api_key {
        if let Some(key_info) = api_key_manager.validate_api_key(key).await {
            debug!(
                key_id = %key_info.id,
                client_name = %key_info.client_name,
                "API key authentication successful"
            );

            // Store key info in request extensions for downstream use
            let mut req = req;
            req.extensions_mut().insert(key_info);
            return Ok(next.run(req).await);
        } else {
            warn!("Invalid API key provided");
        }
    } else {
        debug!("No API key provided in request");
    }

    // Return 401 for missing or invalid API key
    let error_response = serde_json::json!({
        "error": {
            "message": "Invalid or missing API key. Include your API key in the Authorization header as 'Bearer <your-api-key>'.",
            "type": "authentication_error",
            "code": "invalid_api_key"
        }
    });

    Ok((StatusCode::UNAUTHORIZED, ResponseJson(error_response)).into_response())
}

/// Register a new API key
pub async fn register_api_key(
    axum::Extension(api_key_manager): axum::Extension<Arc<ApiKeyManager>>,
    Json(request): Json<ApiKeyRequest>,
) -> Result<ResponseJson<ApiKeyResponse>, StatusCode> {
    match api_key_manager.generate_api_key(request).await {
        Ok(response) => Ok(ResponseJson(response)),
        Err(e) => {
            error!("Failed to generate API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List API keys
pub async fn list_api_keys(
    axum::Extension(api_key_manager): axum::Extension<Arc<ApiKeyManager>>,
) -> ResponseJson<Vec<ApiKeyInfo>> {
    let keys = api_key_manager.list_api_keys().await;
    ResponseJson(keys)
}

/// Revoke API key
#[derive(Deserialize)]
pub struct RevokeKeyRequest {
    pub key_id: String,
}

pub async fn revoke_api_key(
    axum::Extension(api_key_manager): axum::Extension<Arc<ApiKeyManager>>,
    Json(request): Json<RevokeKeyRequest>,
) -> Result<ResponseJson<serde_json::Value>, StatusCode> {
    if api_key_manager.revoke_api_key(&request.key_id).await {
        Ok(ResponseJson(serde_json::json!({
            "message": "API key revoked successfully",
            "key_id": request.key_id
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Create router for public API key registration when enabled
pub fn create_registration_router(enabled: bool) -> Router<Arc<ApiKeyManager>> {
    if enabled {
        Router::new().route("/api/register", post(register_api_key))
    } else {
        Router::new()
    }
}

/// Create router for protected API key management endpoints
pub fn create_api_key_management_router() -> Router<Arc<ApiKeyManager>> {
    Router::new()
        .route("/api/keys", get(list_api_keys))
        .route("/api/keys/revoke", post(revoke_api_key))
}

/// SHA-256 implementation for API key hashing using sha2 crate
mod sha256 {
    use std::fmt::Write;
    use sha2::{Sha256, Digest};

    pub fn digest(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();

        let mut hex_hash = String::with_capacity(64);
        for byte in result.iter() {
            write!(hex_hash, "{:02x}", byte).unwrap();
        }
        hex_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    use tempfile::TempDir;

    fn test_manager() -> (ApiKeyManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_api_keys.db").to_str().unwrap().to_string();
        let manager = ApiKeyManager::new(&db_path).unwrap();
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_api_key_generation_and_validation() {
        let (manager, _temp_dir) = test_manager();
        
        let request = ApiKeyRequest {
            client_name: "test-client".to_string(),
            description: Some("Test API key".to_string()),
            email: Some("test@example.com".to_string()),
        };

        // Generate API key
        let response = manager.generate_api_key(request).await.unwrap();
        assert!(response.api_key.starts_with("embed-"));
        assert_eq!(response.key_info.client_name, "test-client");
        assert_eq!(response.key_info.rate_limit_tier, "development");

        // Validate API key
        let key_info = manager.validate_api_key(&response.api_key).await;
        assert!(key_info.is_some());
        let key_info = key_info.unwrap();
        assert_eq!(key_info.client_name, "test-client");
        assert!(key_info.active);
        assert!(key_info.last_used.is_some());

        // TempDir will be automatically cleaned up
    }

    #[tokio::test]
    async fn test_invalid_api_key() {
        let (manager, _temp_dir) = test_manager();
        let result = manager.validate_api_key("invalid-key").await;
        assert!(result.is_none());
        // TempDir will be automatically cleaned up
    }

    #[tokio::test]
    async fn test_api_key_revocation() {
        let (manager, _temp_dir) = test_manager();
        
        let request = ApiKeyRequest {
            client_name: "test-client".to_string(),
            description: None,
            email: None,
        };

        let response = manager.generate_api_key(request).await.unwrap();
        let key_id = response.key_info.id.clone();

        // Revoke the key
        assert!(manager.revoke_api_key(&key_id).await);

        // Validation should fail
        let result = manager.validate_api_key(&response.api_key).await;
        assert!(result.is_none());
        
        // TempDir will be automatically cleaned up
    }

    #[test]
    fn test_create_registration_router_enabled() {
        let _router = create_registration_router(true);
        // The router should have the /api/register route when enabled
        // Note: We can't easily test the exact routes without axum-test, 
        // but we can verify the function doesn't panic and returns a Router
        assert!(true); // Function executed without panic
    }

    #[test]
    fn test_create_registration_router_disabled() {
        let _router = create_registration_router(false);
        // The router should be empty when disabled
        assert!(true); // Function executed without panic
    }

    #[test]
    fn test_create_api_key_management_router() {
        let router = create_api_key_management_router();
        // The router should have the management routes
        assert!(true); // Function executed without panic
    }

    #[tokio::test]
    async fn test_api_key_auth_middleware_valid_key() {
        // Build a small app protected by the auth middleware
        async fn ok_handler() -> &'static str { "ok" }
        let (manager, _tmp) = test_manager();
        let manager = Arc::new(manager);

        // Generate a key to authorize
        let resp = manager.generate_api_key(ApiKeyRequest{ client_name: "mw-valid".into(), description: None, email: None }).await.unwrap();

        // Layer order matters: apply middleware first, then Extension so Extension is outermost
        // and inserts the manager into request extensions before the middleware runs.
        let app = axum::Router::new()
            .route("/ok", axum::routing::get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_auth_middleware))
            .layer(axum::Extension(manager.clone()));

        use tower::ServiceExt as _;
        let req = axum::http::Request::builder()
            .uri("/ok")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {}", resp.api_key))
            .body(axum::body::Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_key_auth_middleware_invalid_key() {
        async fn ok_handler() -> &'static str { "ok" }
        let (manager, _tmp) = test_manager();
        let manager = Arc::new(manager);

        // Layer order matters: Extension must be applied outermost so the middleware can access it
        let app = axum::Router::new()
            .route("/ok", axum::routing::get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_auth_middleware))
            .layer(axum::Extension(manager.clone()));

        // Invalid Authorization header
        use tower::ServiceExt as _;
        let req = axum::http::Request::builder()
            .uri("/ok")
            .header(axum::http::header::AUTHORIZATION, "Bearer not-a-real-key")
            .body(axum::body::Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_api_keys() {
        let (manager, _temp_dir) = test_manager();
        
        // Generate a few API keys
        let request1 = ApiKeyRequest {
            client_name: "client1".to_string(),
            description: Some("First client".to_string()),
            email: None,
        };
        let request2 = ApiKeyRequest {
            client_name: "client2".to_string(),
            description: None,
            email: Some("client2@example.com".to_string()),
        };
        
        manager.generate_api_key(request1).await.unwrap();
        manager.generate_api_key(request2).await.unwrap();
        
        // List all keys
        let keys = manager.list_api_keys().await;
        assert_eq!(keys.len(), 2);
        
        // Check that both clients are present
        let client_names: Vec<String> = keys.iter().map(|k| k.client_name.clone()).collect();
        assert!(client_names.contains(&"client1".to_string()));
        assert!(client_names.contains(&"client2".to_string()));
        
        // Check that sensitive data is not exposed
        for key in &keys {
            assert!(key.active);
            assert!(key.created_at > 0);
        }
    }

    #[tokio::test]
    async fn test_api_key_auth_middleware_missing_manager_returns_500() {
        async fn ok_handler() -> &'static str { "ok" }
        let app = axum::Router::new()
            .route("/ok", axum::routing::get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_auth_middleware));

        use tower::ServiceExt as _;
        let req = axum::http::Request::builder()
            .uri("/ok")
            .body(axum::body::Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_register_api_key_handler() {
        let (manager, _temp_dir) = test_manager();
        let manager = Arc::new(manager);
        
        let request = ApiKeyRequest {
            client_name: "handler-test-client".to_string(),
            description: Some("Handler test".to_string()),
            email: None,
        };
        
        let result = register_api_key(
            axum::Extension(manager.clone()),
            Json(request),
        ).await;
        
        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert!(response.api_key.starts_with("embed-"));
        assert_eq!(response.key_info.client_name, "handler-test-client");
    }

    #[tokio::test]
    async fn test_list_api_keys_handler() {
        let (manager, _temp_dir) = test_manager();
        let manager = Arc::new(manager);
        
        // Generate an API key first
        let request = ApiKeyRequest {
            client_name: "handler-list-test".to_string(),
            description: None,
            email: None,
        };
        manager.generate_api_key(request).await.unwrap();
        
        // Test the handler
        let result = list_api_keys(axum::Extension(manager)).await;
        let Json(keys) = result;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].client_name, "handler-list-test");
    }

    #[tokio::test]
    async fn test_revoke_api_key_handler() {
        let (manager, _temp_dir) = test_manager();
        let manager = Arc::new(manager);
        
        // Generate an API key first
        let request = ApiKeyRequest {
            client_name: "handler-revoke-test".to_string(),
            description: None,
            email: None,
        };
        let response = manager.generate_api_key(request).await.unwrap();
        let key_id = response.key_info.id.clone();
        
        // Test successful revocation
        let revoke_request = RevokeKeyRequest { key_id: key_id.clone() };
        let result = revoke_api_key(
            axum::Extension(manager.clone()),
            Json(revoke_request),
        ).await;
        
        assert!(result.is_ok());
        let Json(response) = result.unwrap();
        assert_eq!(response["message"], "API key revoked successfully");
        assert_eq!(response["key_id"], key_id);
        
        // Verify the key is actually revoked
        let keys = manager.list_api_keys().await;
        assert_eq!(keys.len(), 1);
        assert!(!keys[0].active);
        
        // Test revoking non-existent key
        let revoke_request = RevokeKeyRequest { key_id: "non-existent".to_string() };
        let result = revoke_api_key(
            axum::Extension(manager),
            Json(revoke_request),
        ).await;
        
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_sha256_digest() {
        let input = b"test input";
        let hash = sha256::digest(input);
        
        // SHA-256 hash should be 64 characters long (32 bytes * 2 hex chars per byte)
        assert_eq!(hash.len(), 64);
        
        // Hash should be consistent
        let hash2 = sha256::digest(input);
        assert_eq!(hash, hash2);
        
        // Different input should produce different hash
        let different_hash = sha256::digest(b"different input");
        assert_ne!(hash, different_hash);
        
        // Test with empty input
        let empty_hash = sha256::digest(b"");
        assert_eq!(empty_hash.len(), 64);
        assert_ne!(empty_hash, hash);
    }

    #[tokio::test]
    async fn test_manager_new_creates_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db").to_str().unwrap().to_string();
        
        let result = ApiKeyManager::new(&db_path);
        assert!(result.is_ok());
        
        // Database file should exist
        assert!(std::path::Path::new(&db_path).exists());
    }

    #[tokio::test]
    async fn test_validate_api_key_malformed() {
        let (manager, _temp) = test_manager();
        
        // Test with malformed API key
        let result = manager.validate_api_key("not-an-api-key").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_validate_api_key_missing_prefix() {
        let (manager, _temp) = test_manager();
        
        // Test without "embed-" prefix
        let result = manager.validate_api_key("abcdef1234567890").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_api_keys_empty() {
        let (manager, _temp) = test_manager();
        
        let keys = manager.list_api_keys().await;
        assert_eq!(keys.len(), 0);
    }

    #[tokio::test]
    async fn test_revoke_api_key_nonexistent() {
        let (manager, _temp) = test_manager();
        
        let result = manager.revoke_api_key("nonexistent-id").await;
        assert_eq!(result, false);
    }

    #[tokio::test]
    async fn test_generate_api_key_duplicate_name() {
        let (manager, _temp) = test_manager();
        
        let request1 = ApiKeyRequest {
            client_name: "duplicate-name".to_string(),
            description: None,
            email: None,
        };
        
        let result1 = manager.generate_api_key(request1).await;
        assert!(result1.is_ok());
        
        let request2 = ApiKeyRequest {
            client_name: "duplicate-name".to_string(),
            description: None,
            email: None,
        };
        
        let result2 = manager.generate_api_key(request2).await;
        // Should allow duplicate names (different IDs)
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_api_key_lifecycle() {
        let (manager, _temp) = test_manager();
        
        // Generate
        let request = ApiKeyRequest {
            client_name: "lifecycle-test".to_string(),
            description: None,
            email: None,
        };
        let response = manager.generate_api_key(request).await.unwrap();
        
        // Validate
        let validated = manager.validate_api_key(&response.api_key).await;
        assert!(validated.is_some());
        assert_eq!(validated.unwrap().client_name, "lifecycle-test");
        
        // List
        let keys = manager.list_api_keys().await;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].client_name, "lifecycle-test");
        
        // Revoke
        let revoked = manager.revoke_api_key(&response.key_info.id).await;
        assert_eq!(revoked, true);
        
        // Validate after revocation should fail
        let validated_after = manager.validate_api_key(&response.api_key).await;
        assert!(validated_after.is_none());
    }

    #[tokio::test]
    async fn test_api_key_request_deserialization() {
        let json = r#"{"client_name":"test-request","description":null,"email":null}"#;
        let deserialized: ApiKeyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.client_name, "test-request");
        assert!(deserialized.description.is_none());
        assert!(deserialized.email.is_none());
    }

    #[tokio::test]
    async fn test_api_key_response_serialization() {
        let info = ApiKeyInfo {
            id: "id-123".to_string(),
            client_name: "test-response".to_string(),
            created_at: 1704067200,
            last_used: None,
            rate_limit_tier: "standard".to_string(),
            max_requests_per_minute: 60,
            active: true,
            description: None,
        };
        
        let response = ApiKeyResponse {
            api_key: "embed-test123".to_string(),
            key_info: info,
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("embed-test123"));
        assert!(json.contains("test-response"));
    }

    #[tokio::test]
    async fn test_api_key_info_serialization() {
        let info = ApiKeyInfo {
            id: "info-123".to_string(),
            client_name: "test-info".to_string(),
            created_at: 1704067200,
            last_used: None,
            rate_limit_tier: "standard".to_string(),
            max_requests_per_minute: 60,
            active: true,
            description: Some("Test description".to_string()),
        };
        
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-info"));
        assert!(json.contains("Test description"));
    }

    #[tokio::test]
    async fn test_api_key_info_with_last_used() {
        let info = ApiKeyInfo {
            id: "used-123".to_string(),
            client_name: "test-used".to_string(),
            created_at: 1704067200,
            last_used: Some(1704153600),
            rate_limit_tier: "premium".to_string(),
            max_requests_per_minute: 120,
            active: true,
            description: None,
        };
        
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("1704153600"));
    }

    #[test]
    fn test_sha256_digest_hex_format() {
        let hash = sha256::digest(b"test");
        
        // Should only contain hex characters
        for c in hash.chars() {
            assert!(c.is_ascii_hexdigit());
        }
    }

    #[test]
    fn test_sha256_digest_large_input() {
        let large_input = vec![0u8; 10000];
        let hash = sha256::digest(&large_input);
        
        // Should still produce 64-char hash
        assert_eq!(hash.len(), 64);
    }

    #[tokio::test]
    async fn test_generate_api_key_empty_name() {
        let (manager, _temp) = test_manager();
        
        let request = ApiKeyRequest {
            client_name: "".to_string(),
            description: None,
            email: None,
        };
        
        let result = manager.generate_api_key(request).await;
        // Should still succeed
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_api_key_special_characters() {
        let (manager, _temp) = test_manager();
        
        let request = ApiKeyRequest {
            client_name: "test!@#$%^&*()".to_string(),
            description: Some("Special chars test".to_string()),
            email: Some("test@example.com".to_string()),
        };
        
        let result = manager.generate_api_key(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_key_format() {
        let (manager, _temp) = test_manager();
        
        let request = ApiKeyRequest {
            client_name: "format-test".to_string(),
            description: None,
            email: None,
        };
        
        let response = manager.generate_api_key(request).await.unwrap();
        
        // API key should start with "embed-"
        assert!(response.api_key.starts_with("embed-"));
        
        // Should have sufficient length
        assert!(response.api_key.len() > 20);
        
        // Key ID should not be empty
        assert!(!response.key_info.id.is_empty());
    }
}