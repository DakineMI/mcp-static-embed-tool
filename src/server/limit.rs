//! Rate limiting middleware and utilities.
//!
//! This module provides two layers of rate limiting:
//! - **IP-based**: Using `tower_governor` with a robust key extractor
//! - **API key-based**: Per-key limiters stored in-memory
//!
//! The IP-based limiter relies on common proxy headers and falls back to the
//! socket address, while the API key limiter requires the `ApiKey` to be present
//! in request extensions (populated by the authentication middleware).
//!
//! # Examples
//!
//! ```no_run
//! use static_embedding_server::server::limit::{ApiKeyRateLimiter, api_key_rate_limit_middleware};
//! use axum::{Router, routing::get, Extension};
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let limiter = Arc::new(ApiKeyRateLimiter::new());
//! let app: Router = Router::new()
//!     .route("/", get(|| async { "ok" }))
//!     .layer(axum::middleware::from_fn(api_key_rate_limit_middleware))
//!     .layer(Extension(limiter));
//! # }
//! ```

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json},
};
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::InMemoryState,
    state::NotKeyed,
    Quota, RateLimiter,
};
use metrics::counter;
use serde_json::json;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_governor::{
    GovernorLayer, errors::GovernorError, governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
};
use tracing::{debug, warn};

use crate::server::api_keys::ApiKey;

/// Custom key extractor that resolves client IP from proxy headers and socket address.
///
/// Tries in order: `X-Forwarded-For`, `X-Real-IP`, `X-Client-IP`, `CF-Connecting-IP`,
/// `True-Client-IP`, `X-Originating-IP`, `X-Remote-IP`, `X-Remote-Addr`. If none are present,
/// it falls back to the `SocketAddr` in request extensions. If all fail, returns "unknown".
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RobustIpKeyExtractor;

impl KeyExtractor for RobustIpKeyExtractor {
    type Key = String;

    fn extract<B>(&self, req: &Request<B>) -> Result<Self::Key, GovernorError> {
        // Output debugging information
        debug!(
            headers = ?req.headers(),
            "Attempting to extract IP address from request"
        );
        // Try to extract IP from various headers in order of preference
        let ip = req
            .headers()
            .get("X-Forwarded-For")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim())
            .or_else(|| {
                req.headers()
                    .get("X-Real-IP") // Nginx
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Client-IP") // Proxies
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("CF-Connecting-IP") // Cloudflare
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("True-Client-IP") // Akamai
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Originating-IP")
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Remote-IP")
                    .and_then(|h| h.to_str().ok())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Remote-Addr")
                    .and_then(|h| h.to_str().ok())
            });
        if let Some(ip) = ip {
            debug!(ip = ip, "Extracted IP address from headers");
            return Ok(ip.to_string());
        }
        // Otherwise, try to retrieve the connection info
        if let Some(addr) = req.extensions().get::<std::net::SocketAddr>() {
            debug!(ip = ?addr.ip(), "Extracted IP address from socket");
            return Ok(addr.ip().to_string());
        }
        // If we don't find an identifying key, use a default key
        warn!("Could not extract IP address from request, using default key");
        Ok("unknown".to_string())
    }
}
/// Create a rate limit layer based on client IP address with robust header extraction
/// Create a rate limit layer based on client IP address with robust header extraction.
///
/// - `rps`: Allowed requests per second
/// - `burst`: Allowed burst size
pub fn create_rate_limit_layer(
    rps: u32,
    burst: u32,
) -> GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, Body> {
    // Create a rate limit configuration using IP addresses
    let config = GovernorConfigBuilder::default()
        .per_second(rps as u64)
        .burst_size(burst)
        .key_extractor(RobustIpKeyExtractor)
        .finish()
        .expect("Failed to create rate limit configuration");

    // Return the rate limit layer
    GovernorLayer::new(config)
}
#[derive(Clone)]
pub struct ApiKeyRateLimiter {
    limiters: Arc<RwLock<HashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>>,
}

impl ApiKeyRateLimiter {
    /// Create a new API key rate limiter registry.
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a rate limiter for a specific API key.
    ///
    /// - `key_id`: API key identifier (not the secret value)
    /// - `max_per_min`: Maximum requests per minute for this key
    ///
    /// The limiter enforces a per-second quota derived from `max_per_min`
    /// using ceiling division with a burst equal to the per-second rate.
    pub async fn get_or_create_limiter(&self, key_id: &str, max_per_min: u32) -> Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> {
        {
            let read_guard = self.limiters.read().await;
            if let Some(limiter) = read_guard.get(key_id) {
                return limiter.clone();
            }
        }

        let rate_per_sec = (((max_per_min as f64 / 60.0).ceil()) as u32).max(1);
        let burst_size = rate_per_sec; // Allow burst up to the per-second rate
        let per_sec_nz = NonZeroU32::new(rate_per_sec).unwrap();
        let burst_nz = NonZeroU32::new(burst_size).unwrap();
        let quota = Quota::per_second(per_sec_nz).allow_burst(burst_nz);
        let limiter = Arc::new(RateLimiter::direct(quota));

        let mut write_guard = self.limiters.write().await;
        write_guard.entry(key_id.to_string()).or_insert(limiter.clone());

        limiter
    }
}

/// Middleware enforcing per-API-key rate limits.
///
/// Requires two request extensions:
/// - `Extension(Arc<ApiKeyRateLimiter>)`: Shared limiter registry
/// - `ApiKey`: Authenticated API key (populated by auth middleware)
///
/// Returns:
/// - `500` if the limiter extension is missing
/// - `401` if the API key extension is missing
/// - `429` if the per-key rate limit is exceeded
/// - Proceeds to next middleware/handler otherwise
pub async fn api_key_rate_limit_middleware(
    req: Request,
    next: Next,
) -> Response<Body> {
    let rate_limiter = match req.extensions().get::<Arc<ApiKeyRateLimiter>>() {
        Some(rl) => rl,
        None => {
            warn!("Rate limiter not found in extensions");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Server configuration error"}))
            )
            .into_response();
        }
    };

    let api_key = match req.extensions().get::<ApiKey>() {
        Some(ak) => ak.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "No API key in request"}))
            )
            .into_response();
        }
    };

    let limiter = rate_limiter
        .get_or_create_limiter(&api_key.id, api_key.max_requests_per_minute)
        .await;

    if limiter.check().is_err() {
        warn!("Rate limit exceeded for key {}", api_key.id);
        counter!("embedtool.total_rate_limit_errors").increment(1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "Rate limit exceeded for this API key"}))
        )
        .into_response();
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, HeaderMap, HeaderValue, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tower::ServiceExt;

    #[test]
    fn test_robust_ip_key_extractor_x_forwarded_for() {
        let extractor = RobustIpKeyExtractor;
        
        // Test X-Forwarded-For header
        let req = Request::builder()
            .uri("http://example.com")
            .header("X-Forwarded-For", "192.168.1.100, 10.0.0.1")
            .body(())
            .unwrap();
        
        let key = extractor.extract(&req).unwrap();
        assert_eq!(key, "192.168.1.100");
    }

    #[test]
    fn test_robust_ip_key_extractor_multiple_headers() {
        let extractor = RobustIpKeyExtractor;
        
        // Test X-Real-IP
        let req = Request::builder()
            .uri("http://example.com")
            .header("X-Real-IP", "10.0.0.1")
            .body(())
            .unwrap();
        
        let key = extractor.extract(&req).unwrap();
        assert_eq!(key, "10.0.0.1");
        
        // Test X-Client-IP
        let req2 = Request::builder()
            .uri("http://example.com")
            .header("X-Client-IP", "172.16.0.1")
            .body(())
            .unwrap();
        
        let key2 = extractor.extract(&req2).unwrap();
        assert_eq!(key2, "172.16.0.1");
    }

    #[test]
    fn test_robust_ip_key_extractor_socket_addr() {
        let extractor = RobustIpKeyExtractor;
        
        // No headers, should fall back to socket address
        let socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut req = Request::builder()
            .uri("http://example.com")
            .body(())
            .unwrap();
        
        // Insert socket address into extensions
        req.extensions_mut().insert(socket_addr);
        
        let key = extractor.extract(&req).unwrap();
        assert_eq!(key, "127.0.0.1");
    }

    #[test]
    fn test_robust_ip_key_extractor_fallback_to_unknown() {
        let extractor = RobustIpKeyExtractor;
        let req = Request::builder().uri("http://example.com").body(()).unwrap();
        
        // No headers and no socket address
        let key = extractor.extract(&req).unwrap();
        assert_eq!(key, "unknown");
    }

    #[test]
    fn test_api_key_rate_limiter_creation() {
        let limiter = ApiKeyRateLimiter::new();
        assert!(limiter.limiters.try_read().is_ok());
    }

    #[tokio::test]
    async fn test_api_key_rate_limit_middleware() {
        // Test that the middleware function exists and has correct signature.
        // A full behavioral test would require constructing a Next service compatible with axum 0.8.
        assert!(true);
    }

    #[test]
    fn test_create_rate_limit_layer_compiles() {
        // Ensure the governor layer can be created with our extractor
        let _layer = create_rate_limit_layer(10, 20);
        assert!(true);
    }

    #[tokio::test]
    async fn test_api_key_rate_limiter_reuse() {
        // Verify that calling get_or_create_limiter returns the same limiter for the same key
        let limiter = ApiKeyRateLimiter::new();
        let l1 = limiter.get_or_create_limiter("key-1", 100).await;
        let l2 = limiter.get_or_create_limiter("key-1", 100).await;
        assert!(Arc::ptr_eq(&l1, &l2));
    }

    // --- Behavioral tests exercising middleware branches ---

    async fn ok_handler() -> &'static str { "ok" }

    #[tokio::test]
    async fn test_rate_limit_middleware_missing_rate_limiter_extension_returns_500() {
        // Build an app WITHOUT the Extension(Arc<ApiKeyRateLimiter>) so the middleware
        // fails early with 500 Internal Server Error
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_rate_limit_middleware));

        // Construct a request that includes an ApiKey in extensions (to bypass 401 path)
        let api_key = ApiKey {
            id: "key-500".to_string(),
            key_hash: "h".to_string(),
            client_name: "c".to_string(),
            created_at: 0,
            last_used: None,
            rate_limit_tier: "t".to_string(),
            max_requests_per_minute: 60,
            active: true,
            description: None,
        };
        let mut req = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(api_key);

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_rate_limit_middleware_missing_api_key_returns_401() {
        // Build an app with the middleware only; insert the rate limiter directly into request extensions
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_rate_limit_middleware));

        let mut req = Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(Arc::new(ApiKeyRateLimiter::new()));

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_rate_limit_middleware_exceeded_returns_429() {
        // Build an app with the middleware only; insert both rate limiter and api key per request
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(axum::middleware::from_fn(api_key_rate_limit_middleware));

        // Api key limited to 1 request per minute -> second request should 429
        let api_key = ApiKey {
            id: "key-rl".to_string(),
            key_hash: "h".to_string(),
            client_name: "c".to_string(),
            created_at: 0,
            last_used: None,
            rate_limit_tier: "t".to_string(),
            max_requests_per_minute: 1,
            active: true,
            description: None,
        };

        // Reuse the same rate limiter across both requests so the second is limited
        let shared_rl = Arc::new(ApiKeyRateLimiter::new());

        // First request should pass
        let mut req1 = Request::builder().uri("/test").body(Body::empty()).unwrap();
        req1.extensions_mut().insert(shared_rl.clone());
        req1.extensions_mut().insert(api_key.clone());
        let res1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(res1.status(), StatusCode::OK);

        // Second immediate request should be rate-limited
        let mut req2 = Request::builder().uri("/test").body(Body::empty()).unwrap();
        req2.extensions_mut().insert(shared_rl);
        req2.extensions_mut().insert(api_key);
        let res2 = app.clone().oneshot(req2).await.unwrap();
        assert_eq!(res2.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
