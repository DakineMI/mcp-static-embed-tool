use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json},
};
use governor::{
    middleware::NoOpMiddleware,
    clock::DefaultClock,
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

/// Custom key extractor that tries to get IP from various headers and falls back to a default
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
pub fn create_rate_limit_layer(rps: u32, burst: u32) -> GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, Arc<HashMap<String, InMemoryState>>> {
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
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

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
    use axum::http::{Request, HeaderMap, HeaderValue};
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
    async fn test_api_key_rate_limiter_get_or_create() {
        let limiter = ApiKeyRateLimiter::new();
        
        // First call should create a new limiter
        let limiter1 = limiter.get_or_create_limiter("test-key", 60).await;
        assert!(limiter1.check().is_ok()); // Should allow the request
        
        // Second call should return the same limiter
        let limiter2 = limiter.get_or_create_limiter("test-key", 60).await;
        assert!(Arc::ptr_eq(&limiter1, &limiter2));
    }

    #[tokio::test]
    async fn test_api_key_rate_limiter_different_configs() {
        let limiter = ApiKeyRateLimiter::new();
        
        // Create limiters with different rates
        let limiter1 = limiter.get_or_create_limiter("key1", 60).await; // 1 req/sec
        let limiter2 = limiter.get_or_create_limiter("key2", 120).await; // 2 req/sec
        
        // They should be different instances
        assert!(!Arc::ptr_eq(&limiter1, &limiter2));
    }
}
