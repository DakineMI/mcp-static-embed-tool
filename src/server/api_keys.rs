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
use std::env;
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
        let mut rng = rand::thread_rng();
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

    fn test_manager() -> (ApiKeyManager, String) {
        use uuid::Uuid;
        let db_path = format!("./test_api_keys_{}.db", Uuid::new_v4());
        let manager = ApiKeyManager::new(&db_path).unwrap();
        (manager, db_path)
    }

    #[tokio::test]
    async fn test_api_key_generation_and_validation() {
        let (manager, db_path) = test_manager();
        
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

        // Cleanup
        drop(manager);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_invalid_api_key() {
        let (manager, db_path) = test_manager();
        let result = manager.validate_api_key("invalid-key").await;
        assert!(result.is_none());
        drop(manager);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn test_api_key_revocation() {
        let (manager, db_path) = test_manager();
        
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
        
        drop(manager);
        let _ = std::fs::remove_file(&db_path);
    }
}