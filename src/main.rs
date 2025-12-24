#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run the CLI
    static_embedding_tool::cli::run_cli().await
}