

use tracing::info;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use metrics::{counter, gauge};

/// Initialize structured logging and metrics collection.
///
/// Sets up tracing subscriber with environment-based filtering and configures
/// output based on server mode (HTTP vs STDIO).
///
/// # Arguments
///
/// * `stdio` - If true, logs to stderr (for MCP STDIO mode); otherwise stdout
///
/// # Environment Variables
///
/// - `RUST_LOG`: Controls log level filtering (e.g., "info", "debug")
///
/// # Examples
///
/// ```no_run
/// # use static_embedding_tool::logs::init_logging_and_metrics;
/// // Initialize for HTTP mode
/// init_logging_and_metrics(false);
///
/// // Initialize for STDIO mode
/// init_logging_and_metrics(true);
/// ```
pub fn init_logging_and_metrics(stdio: bool) {
    {
        // Check if we are running in stdio mode
        if stdio {
            // Set up environment filter for log levels
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("static_embedding_tool=error,rmcp=error"));
            // Initialize tracing subscriber with stderr output
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_writer(std::io::stderr),
                )
                .try_init(); // Use try_init to avoid panic if already initialized
        } else {
            // Set up environment filter for log levels
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("static_embedding_tool=trace,rmcp=warn"));
            // Initialize tracing subscriber with stdout output
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(true)
                        .with_writer(std::io::stdout),
                )
                .try_init(); // Use try_init to avoid panic if already initialized
        }
        // Output debugging information
        info!("Logging and tracing initialized");
    }

    {
        // Initialize metrics with default values
        gauge!("embedtool.active_connections").set(0.0);
        counter!("embedtool.total_connections").absolute(0);
        counter!("embedtool.total_embedding_requests").absolute(0);
        // Error metrics - general
        counter!("embedtool.total_errors").absolute(0);
        // Error metrics - specific categories
        counter!("embedtool.total_embedding_errors").absolute(0);
        counter!("embedtool.total_connection_errors").absolute(0);
        counter!("embedtool.total_configuration_errors").absolute(0);
         // Operation-specific error metrics
        counter!("embedtool.errors.model_load").absolute(0);
        counter!("embedtool.errors.embedding_generation").absolute(0);
        counter!("embedtool.errors.batch_processing").absolute(0);
        counter!("embedtool.errors.model_distillation").absolute(0);
        counter!("embedtool.errors.model_not_found").absolute(0);
        counter!("embedtool.errors.invalid_input").absolute(0);
        // Tool method call counters
        counter!("embedtool.tools.embed").absolute(0);
        counter!("embedtool.tools.batch_embed").absolute(0);
        counter!("embedtool.tools.list_models").absolute(0);
        counter!("embedtool.tools.model_info").absolute(0);
        counter!("embedtool.tools.distill_model").absolute(0);
        // Output debugging information
        info!("Metrics collection initialized");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_logging_and_metrics_stdio_true() {
        // Test that the function can be called without panicking
        // We use try_init internally to avoid conflicts with other tests
        let result = std::panic::catch_unwind(|| {
            init_logging_and_metrics(true);
        });
        assert!(result.is_ok(), "init_logging_and_metrics(true) should not panic");
    }

    #[test]
    fn test_init_logging_and_metrics_stdio_false() {
        // Test that the function can be called without panicking
        let result = std::panic::catch_unwind(|| {
            init_logging_and_metrics(false);
        });
        assert!(result.is_ok(), "init_logging_and_metrics(false) should not panic");
    }

    #[test]
    fn test_metrics_initialization() {
        // Test metrics initialization separately without calling init_logging_and_metrics
        // to avoid global subscriber conflicts
        
        // Initialize metrics directly
        use metrics::{counter, gauge};
        gauge!("embedtool.active_connections").set(0.0);
        counter!("embedtool.total_connections").absolute(0);
        counter!("embedtool.total_embedding_requests").absolute(0);
        counter!("embedtool.total_errors").absolute(0);
        counter!("embedtool.total_embedding_errors").absolute(0);
        counter!("embedtool.total_connection_errors").absolute(0);
        counter!("embedtool.total_configuration_errors").absolute(0);
        counter!("embedtool.errors.model_load").absolute(0);
        counter!("embedtool.errors.embedding_generation").absolute(0);
        counter!("embedtool.errors.batch_processing").absolute(0);
        counter!("embedtool.errors.model_distillation").absolute(0);
        counter!("embedtool.errors.model_not_found").absolute(0);
        counter!("embedtool.errors.invalid_input").absolute(0);
        counter!("embedtool.tools.embed").absolute(0);
        counter!("embedtool.tools.batch_embed").absolute(0);
        counter!("embedtool.tools.list_models").absolute(0);
        counter!("embedtool.tools.model_info").absolute(0);
        counter!("embedtool.tools.distill_model").absolute(0);
        
        // Test that we can increment counters (they should exist)
        counter!("embedtool.total_connections").increment(1);
        counter!("embedtool.total_embedding_requests").increment(1);
        counter!("embedtool.total_errors").increment(1);
        
        // Test error category counters
        counter!("embedtool.total_embedding_errors").increment(1);
        counter!("embedtool.total_connection_errors").increment(1);
        counter!("embedtool.total_configuration_errors").increment(1);
        
        // Test operation-specific error counters
        counter!("embedtool.errors.model_load").increment(1);
        counter!("embedtool.errors.embedding_generation").increment(1);
        counter!("embedtool.errors.batch_processing").increment(1);
        counter!("embedtool.errors.model_distillation").increment(1);
        counter!("embedtool.errors.model_not_found").increment(1);
        counter!("embedtool.errors.invalid_input").increment(1);
        
        // Test tool method counters
        counter!("embedtool.tools.embed").increment(1);
        counter!("embedtool.tools.batch_embed").increment(1);
        counter!("embedtool.tools.list_models").increment(1);
        counter!("embedtool.tools.model_info").increment(1);
        counter!("embedtool.tools.distill_model").increment(1);
        
        // Test gauge
        gauge!("embedtool.active_connections").set(5.0);
    }
}
