use axum::http::StatusCode;

/// Health check endpoint for load balancer health status checking
pub async fn health() -> StatusCode {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        let status = health().await;
        assert_eq!(status, StatusCode::OK);
    }
}
