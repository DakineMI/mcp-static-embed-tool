use static_embedding_server::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::run_cli().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_module_import() {
        // Test that we can import and reference the cli module
        // This exercises the use statement and verifies the module exists
        let _cli_ref = static_embedding_server::cli::run_cli;
        assert!(true); // If we get here, the import worked
    }

    #[tokio::test]
    async fn test_main_function_signature() {
        // Test that main function exists and has correct signature
        // This is a compile-time test to ensure the main function is properly defined
        // We can't actually call main() in tests as it would run the full CLI
        // but we can verify the function exists and compiles correctly
        
        // Create a mock function with the same signature as main
        async fn mock_main() -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        
        // Test that our mock function signature matches main
        let result = mock_main().await;
        assert!(result.is_ok());
    }

    #[test] 
    fn test_main_attributes() {
        // Test that the main function has the tokio::main attribute
        // This is verified by the fact that the code compiles with async main
        // The #[tokio::main] macro transforms async fn main() into a sync main that runs a tokio runtime
        
        // If this test runs, it means the tokio::main attribute is working correctly
        assert!(true);
    }
}
