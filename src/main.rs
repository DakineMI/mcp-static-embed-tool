use static_embedding_server::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::run_cli().await
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_cli_module_import() {
        // Test that we can import and reference the cli module
        // This exercises the use statement and verifies the module exists
        let _cli_ref = static_embedding_server::cli::run_cli;
        assert!(true); // If we get here, the import worked
    }
}
