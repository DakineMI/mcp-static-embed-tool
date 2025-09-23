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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use uuid::Uuid;
use rand::Rng;

/// API Key information
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Storage for API keys (in-memory for now, could be database later)
    keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    /// Index by key hash for fast lookup
    key_index: Arc<RwLock<HashMap<String, String>>>, // hash -> id
}

impl ApiKeyManager {
    /// Create a new API key manager
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
            key_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate a new API key
    pub async fn generate_api_key(&self, request: ApiKeyRequest) -> Result<ApiKeyResponse, String> {
        let key_id = Uuid::new_v4().to_string();
        
        // Generate a secure API key: embed-<base64-encoded-random-bytes>
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut random_bytes = [0u8; 32];
        rng.fill_bytes(&mut random_bytes);
        let api_key = format!("embed-{}", STANDARD.encode(random_bytes));
        
        // Hash the API key for storage
        let key_hash = format!("{:x}", sha256::digest(api_key.as_bytes()));
        
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

        // Store the API key
        {
            let mut keys = self.keys.write().await;
            keys.insert(key_id.clone(), api_key_info.clone());
        }

        // Update the index
        {
            let mut index = self.key_index.write().await;
            index.insert(key_hash, key_id.clone());
        }

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
        let key_hash = format!("{:x}", sha256::digest(api_key.as_bytes()));
        
        // Look up the key ID
        let key_id = {
            let index = self.key_index.read().await;
            index.get(&key_hash).cloned()
        }?;

        // Get the key info
        let mut key_info = {
            let keys = self.keys.read().await;
            keys.get(&key_id).cloned()
        }?;

        // Check if key is active
        if !key_info.active {
            return None;
        }

        // Update last used timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        key_info.last_used = Some(now);

        // Update in storage
        {
            let mut keys = self.keys.write().await;
            keys.insert(key_id, key_info.clone());
        }

        debug!(
            key_id = %key_info.id,
            client_name = %key_info.client_name,
            "API key validated successfully"
        );

        Some(key_info)
    }

    /// List all API keys (without sensitive data)
    pub async fn list_api_keys(&self) -> Vec<ApiKeyInfo> {
        let keys = self.keys.read().await;
        keys.values()
            .map(|key| ApiKeyInfo {
                id: key.id.clone(),
                client_name: key.client_name.clone(),
                created_at: key.created_at,
                last_used: key.last_used,
                rate_limit_tier: key.rate_limit_tier.clone(),
                max_requests_per_minute: key.max_requests_per_minute,
                active: key.active,
                description: key.description.clone(),
            })
            .collect()
    }

    /// Revoke an API key
    pub async fn revoke_api_key(&self, key_id: &str) -> bool {
        let mut keys = self.keys.write().await;
        if let Some(mut key) = keys.get(key_id).cloned() {
            key.active = false;
            keys.insert(key_id.to_string(), key);
            info!(key_id = %key_id, "API key revoked");
            true
        } else {
            false
        }
    }
}

/// API key authentication middleware
pub async fn api_key_auth_middleware<B>(
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode>
where
    B: Send,
{
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

/// Create API key management router
pub fn create_api_key_router() -> Router<Arc<ApiKeyManager>> {
    Router::new()
        .route("/register", post(register_api_key))
        .route("/list", get(list_api_keys))
        .route("/revoke", post(revoke_api_key))
}

/// Simple SHA-256 implementation for API key hashing
mod sha256 {
    use std::fmt::Write;

    pub fn digest(data: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let hash = hasher.finish();
        
        format!("{:016x}{:016x}", hash, hash.wrapping_mul(0x9e3779b97f4a7c15))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_api_key_generation_and_validation() {
        let manager = ApiKeyManager::new();
        
        let request = ApiKeyRequest {
            client_name: "test-client".to_string(),
            description: Some("Test API key".to_string()),
            email: Some("test@example.com".to_string()),
        };

        // Generate API key
        let response = manager.generate_api_key(request).await.unwrap();
        assert!(response.api_key.starts_with("embed-"));
        assert_eq!(response.key_info.client_name, "test-client");
        assert_eq!(response.key_info.rate_limit_tier, "standard");

        // Validate API key
        let key_info = manager.validate_api_key(&response.api_key).await;
        assert!(key_info.is_some());
        let key_info = key_info.unwrap();
        assert_eq!(key_info.client_name, "test-client");
        assert!(key_info.active);
        assert!(key_info.last_used.is_some());
    }

    #[tokio::test]
    async fn test_invalid_api_key() {
        let manager = ApiKeyManager::new();
        let result = manager.validate_api_key("invalid-key").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_api_key_revocation() {
        let manager = ApiKeyManager::new();
        
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
    }
}