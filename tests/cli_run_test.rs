use std::sync::Once;

use clap::Parser;
use static_embedding_tool::cli::{Cli, Commands, ServerAction};

static INIT: Once = Once::new();

fn init_logger() {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt().with_max_level(tracing::Level::ERROR).try_init();
    });
}

#[test]
fn parse_server_start_args() {
    init_logger();
    let args = vec!["embed-tool", "server", "start", "--port", "7070", "--bind", "127.0.0.1"];    
    let cli = Cli::try_parse_from(args).unwrap();
    match cli.command {
        Commands::Server { action: ServerAction::Start(start) } => {
            assert_eq!(start.port, 7070);
            assert_eq!(start.bind, "127.0.0.1");
        }
        _ => panic!("expected server start"),
    }
}
