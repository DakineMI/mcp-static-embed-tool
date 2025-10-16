use static_embedding_server::server::errors::AppError;

#[test]
fn app_error_variants_display_and_metadata() {
    // Display
    let e1 = AppError::ModelLoad("m".into(), "x".into());
    assert!(e1.to_string().contains("Failed to load model"));

    // error_type mapping
    assert_eq!(AppError::NoModelsAvailable.error_type(), "server_error");
    assert_eq!(AppError::AuthFailed.error_type(), "authentication_error");
    assert_eq!(AppError::InvalidApiKeyFormat.error_type(), "invalid_request_error");

    // code mapping
    assert_eq!(AppError::InvalidInput("t".into()).code(), Some("invalid_input"));
    assert_eq!(AppError::RateLimitExceeded("k".into()).code(), Some("rate_limit_exceeded"));
    assert_eq!(AppError::ApiKeyNotFound.code(), None);
}
