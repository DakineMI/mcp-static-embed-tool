use static_embedding_tool::server::errors::AppError;

#[test]
fn app_error_variants_display_and_metadata() {
    // Display
    let e1 = AppError::ModelLoad("m".into(), "x".into());
    assert!(e1.to_string().contains("Failed to load model"));

    // error_type mapping
    assert_eq!(AppError::NoModelsAvailable.error_type(), "server_error");
    // code mapping
    assert_eq!(AppError::InvalidInput("t".into()).code(), Some("invalid_input"));
    
}
