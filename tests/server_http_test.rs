// Integration test for server/http.rs
use static_embedding_tool::server::http;

#[tokio::test]
async fn health_returns_ok() {
    let status = http::health().await;
    assert_eq!(status, axum::http::StatusCode::OK);
}
