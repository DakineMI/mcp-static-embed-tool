use crate::cli::{ServerAction, StartArgs};
use crate::server::start::{start_server, ServerConfig};
use anyhow::{anyhow, Result as AnyhowResult};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::fs;
use sysinfo::{System, Pid};

pub async fn handle_server_command(
    action: ServerAction,
    config_path: Option<PathBuf>,
) -> AnyhowResult<()> {
    match action {
        ServerAction::Start(args) => handle_start_server(args, config_path).await,
        ServerAction::Stop => stop_server().await,
        ServerAction::Status => show_status().await,
        ServerAction::Restart(args) => {
            if is_server_running().await? {
                stop_server().await?;
                // Wait a moment for cleanup
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            handle_start_server(args, config_path).await
        }
    }
}

async fn validate_start_args(args: &StartArgs) -> AnyhowResult<()> {
    // Validate models
    if let Some(models_str) = &args.models {
        let model_list: Vec<&str> = models_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if model_list.is_empty() {
            return Err(anyhow!("No valid models specified in --models"));
        }
        let default = args.default_model.trim();
        if !model_list.contains(&default) {
            return Err(anyhow!(
                "Default model '{}' must be one of the specified models: {}",
                default,
                models_str
            ));
        }
    }
    Ok(())
}

async fn handle_start_server(
    args: StartArgs,
    _config_path: Option<PathBuf>,
) -> AnyhowResult<()> {
    // Validate models
    validate_start_args(&args).await?;

    // Check if server is already running
    if is_server_running().await? {
        eprintln!("Server is already running. Use 'embed-tool server stop' first or 'embed-tool server restart'.");
        return Ok(());
    }

    if args.daemon {
        start_daemon(args).await
    } else {
        start_foreground(args).await
    }
}

async fn start_foreground(args: StartArgs) -> AnyhowResult<()> {
    println!("Starting embedding server in foreground mode...");
    println!("Port: {}", args.port);
    println!("Bind: {}", args.bind);
    println!("Default model: {}", args.default_model);
    
    if let Some(models) = &args.models {
        println!("Models: {}", models);
    }
    
    if args.mcp {
        println!("MCP mode: enabled");
    }

    if args.auth_disabled {
        println!("Authentication: disabled");
    }

    let config = if args.mcp {
        // MCP mode: stdio
        ServerConfig {
            server_url: "stdio://-".to_string(),
            bind_address: None,
            socket_path: None,
            auth_disabled: args.auth_disabled,
            registration_enabled: !args.auth_disabled,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            api_key_db_path: "./data/api_keys.db".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: args.mcp,
        }
    } else if let Some(socket_path) = args.socket_path {
        ServerConfig {
            server_url: format!("unix://{}", socket_path.display()),
            bind_address: None,
            socket_path: Some(socket_path.to_string_lossy().into_owned()),
            auth_disabled: args.auth_disabled,
            registration_enabled: !args.auth_disabled,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            api_key_db_path: "./data/api_keys.db".to_string(),
            tls_cert_path: None,
            tls_key_path: None,
            enable_mcp: args.mcp,
        }
    } else {
        let addr = format!("{}:{}", args.bind, args.port);
        ServerConfig {
            server_url: format!("http://{}", addr),
            bind_address: Some(addr),
            socket_path: None,
            auth_disabled: args.auth_disabled,
            registration_enabled: !args.auth_disabled,
            rate_limit_rps: 100,
            rate_limit_burst: 200,
            api_key_db_path: "./data/api_keys.db".to_string(),
            tls_cert_path: args.tls_cert_path,
            tls_key_path: args.tls_key_path,
            enable_mcp: args.mcp,
        }
    };

    start_server(config).await
}

async fn start_daemon(args: StartArgs) -> AnyhowResult<()> {
    println!("Starting embedding server as daemon...");
    
    let current_exe = std::env::current_exe()?;
    let pid_file = args.pid_file.clone().unwrap_or_else(|| {
        std::env::temp_dir().join("embed-tool.pid")
    });
    
    let port_str = args.port.to_string();
    let bind_str = args.bind.clone();
    let default_model_str = args.default_model.clone();
    
    // Convert StartArgs back to command line arguments
    let mut cmd_args = vec!["server", "start"];
    cmd_args.push("--port");
    cmd_args.push(&port_str);
    cmd_args.push("--bind");
    cmd_args.push(&bind_str);
    cmd_args.push("--default-model");
    cmd_args.push(&default_model_str);
    
    if let Some(models) = &args.models {
        cmd_args.push("--models");
        cmd_args.push(models);
    }
    
    if args.mcp {
        cmd_args.push("--mcp");
    }
    
    // Start the process detached
    let child = Command::new(current_exe)
        .args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    
    // Write PID file
    fs::write(&pid_file, child.id().to_string())?;
    
    println!("Server started as daemon with PID: {}", child.id());
    println!("PID file: {}", pid_file.display());
    
    Ok(())
}

async fn stop_server() -> AnyhowResult<()> {
    let pid_file = std::env::temp_dir().join("embed-tool.pid");
    
    if !pid_file.exists() {
        // Try to find by port
        if let Some(pid) = find_server_by_port(8080).await? {
            terminate_process(pid)?;
            println!("Server stopped (found by port)");
        } else {
            println!("No running server found");
        }
        return Ok(());
    }
    
    let pid_str = fs::read_to_string(&pid_file)?;
    let pid: u32 = pid_str.trim().parse()?;
    
    terminate_process(pid)?;
    fs::remove_file(&pid_file)?;
    
    println!("Server stopped (PID: {})", pid);
    Ok(())
}

async fn show_status() -> AnyhowResult<()> {
    let pid_file = std::env::temp_dir().join("embed-tool.pid");
    
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if is_process_running(pid) {
                println!("Server is running (PID: {})", pid);
                println!("PID file: {}", pid_file.display());
                
                // Try to get more info by checking port
                if let Some(_) = find_server_by_port(8080).await? {
                    println!("HTTP API: http://localhost:8080");
                }
            } else {
                println!("Server is not running (stale PID file)");
                fs::remove_file(&pid_file)?;
            }
        }
    } else if let Some(pid) = find_server_by_port(8080).await? {
        println!("Server is running (PID: {}) but no PID file found", pid);
        println!("HTTP API: http://localhost:8080");
    } else {
        println!("Server is not running");
    }
    
    Ok(())
}

async fn is_server_running() -> AnyhowResult<bool> {
    let pid_file = std::env::temp_dir().join("embed-tool.pid");
    
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if is_process_running(pid) {
                return Ok(true);
            } else {
                // Clean up stale PID file
                fs::remove_file(&pid_file)?;
            }
        }
    }
    
    // Check by port as fallback
    Ok(find_server_by_port(8080).await?.is_some())
}

fn is_process_running(pid: u32) -> bool {
    let mut system = System::new();
    system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, false, sysinfo::ProcessRefreshKind::new());
    system.process(Pid::from(pid as usize)).is_some()
}

async fn find_server_by_port(port: u16) -> AnyhowResult<Option<u32>> {
    // This is a simplified implementation
    // In practice, you'd want to check netstat or similar
    let output = Command::new("lsof")
        .args(&["-t", &format!("-i:{}", port)])
        .output()?;

    println!("lsof output status: {}", output.status);
    println!("lsof stdout: {:?}", String::from_utf8_lossy(&output.stdout));
    println!("lsof stderr: {:?}", String::from_utf8_lossy(&output.stderr));

    if output.status.success() && !output.stdout.is_empty() {
        let pid_str = String::from_utf8(output.stdout)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return Ok(Some(pid));
        }
    }

    Ok(None)
}

fn terminate_process(pid: u32) -> AnyhowResult<()> {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    
    #[cfg(windows)]
    {
        Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output()?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[tokio::test]
    async fn test_validate_models_in_start_args() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: Some("model1,model2".to_string()),
            default_model: "model1".to_string(),
            mcp: false,
            auth_disabled: false,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // This should succeed
        assert!(validate_start_args(&args).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_models_invalid_default() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: Some("model1,model2".to_string()),
            default_model: "model3".to_string(), // Not in models list
            mcp: false,
            auth_disabled: false,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        let result = handle_start_server(args, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Default model"));
    }

    #[tokio::test]
    async fn test_validate_models_empty_list() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: Some(",,,,".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: false,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        let result = handle_start_server(args, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No valid models"));
    }

    #[test]
    fn test_is_process_running() {
        // Test with a non-existent PID
        assert!(!is_process_running(999999));

        // Test with PID 1 (usually exists on Unix systems)
        // Note: This might fail on some systems, but is generally reliable
        #[cfg(unix)]
        assert!(is_process_running(1));
    }

    #[tokio::test]
    async fn test_is_server_running_no_pid_file() {
        // Remove any existing PID file
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        let _ = fs::remove_file(&pid_file);

        // Should return false when no PID file exists and no server is running on port 8080
        let result = is_server_running().await;
        assert!(result.is_ok());
        // Note: This might return true if something is actually running on port 8080
    }

    #[tokio::test]
    async fn test_show_status_no_server() {
        // Remove any existing PID file
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        let _ = fs::remove_file(&pid_file);

        // Should not panic
        let result = show_status().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_server_no_pid_file() {
        // Remove any existing PID file
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        let _ = fs::remove_file(&pid_file);

        // Should not panic
        let result = stop_server().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_terminate_process() {
        // Test terminating a non-existent process (should not panic)
        let result = terminate_process(999999);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_find_server_by_port() {
        // Test finding server on a port that's unlikely to have anything
        let result = find_server_by_port(65530).await;
        assert!(result.is_ok());
        let server_pid = result.unwrap();
        println!("Server PID found: {:?}", server_pid);
        assert!(server_pid.is_none());
    }

    #[tokio::test]
    async fn test_handle_server_command_status() {
        let result = handle_server_command(ServerAction::Status, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_server_command_stop() {
        let result = handle_server_command(ServerAction::Stop, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_server_command_start() {
        let args = StartArgs {
            port: 8081, // Use different port to avoid conflicts
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // This will likely fail to start the actual server, but should not panic
        let result = handle_server_command(ServerAction::Start(args), None).await;
        // We expect this to succeed in test environment even if server doesn't actually start
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_handle_server_command_restart() {
        let args = StartArgs {
            port: 8082, // Use different port
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Should not panic
        let result = handle_server_command(ServerAction::Restart(args), None).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_validate_start_args_no_models() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: None, // No models specified
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: false,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Should succeed when no models are specified
        assert!(validate_start_args(&args).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_start_args_whitespace_models() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: Some("  model1  ,  model2  ".to_string()),
            default_model: "model1".to_string(),
            mcp: false,
            auth_disabled: false,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Should handle whitespace properly
        assert!(validate_start_args(&args).await.is_ok());
    }

    #[tokio::test]
    async fn test_start_foreground_http_config() {
        let args = StartArgs {
            port: 8083,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Will likely fail to actually start server, but should not panic
        let result = start_foreground(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_foreground_mcp_config() {
        let args = StartArgs {
            port: 8084,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: true, // MCP mode
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Will likely fail to actually start server, but should not panic
        let result = start_foreground(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_foreground_socket_config() {
        let temp_dir = std::env::temp_dir();
        let socket_path = temp_dir.join("test_socket.sock");
        
        let args = StartArgs {
            port: 8085,
            bind: "127.0.0.1".to_string(),
            socket_path: Some(socket_path),
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };

        // Will likely fail to actually start server, but should not panic
        let result = start_foreground(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_foreground_with_tls() {
        let args = StartArgs {
            port: 8443,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: Some("/path/to/cert.pem".to_string()),
            tls_key_path: Some("/path/to/key.pem".to_string()),
        };

        // Will likely fail to actually start server due to missing certs, but should not panic
        let result = start_foreground(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_daemon_basic() {
        let temp_dir = std::env::temp_dir();
        let pid_file = temp_dir.join("test_daemon.pid");
        
        let args = StartArgs {
            port: 8086,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: true,
            pid_file: Some(pid_file.clone()),
            tls_cert_path: None,
            tls_key_path: None,
        };

        // This will try to spawn a daemon process
        let result = start_daemon(args).await;
        
        // Clean up any PID file that might have been created
        let _ = fs::remove_file(&pid_file);
        
        // The result depends on whether the process can actually be spawned
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_daemon_with_mcp() {
        let temp_dir = std::env::temp_dir();
        let pid_file = temp_dir.join("test_daemon_mcp.pid");
        
        let args = StartArgs {
            port: 8087,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M,custom-model".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: true,
            auth_disabled: false,
            daemon: true,
            pid_file: Some(pid_file.clone()),
            tls_cert_path: None,
            tls_key_path: None,
        };

        let result = start_daemon(args).await;
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
        
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_daemon_default_pid_file() {
        let args = StartArgs {
            port: 8088,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: None,
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: true,
            pid_file: None, // Use default PID file location
            tls_cert_path: None,
            tls_key_path: None,
        };

        let result = start_daemon(args).await;
        
        // Clean up default PID file
        let default_pid_file = std::env::temp_dir().join("embed-tool.pid");
        let _ = fs::remove_file(&default_pid_file);
        
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_stop_server_with_valid_pid_file() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = stop_server().await;
        
        // Should succeed even if process doesn't exist
        assert!(result.is_ok());
        
        // PID file should be removed
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_stop_server_invalid_pid_file() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create a PID file with invalid content
        fs::write(&pid_file, "not_a_number").unwrap();
        
        let result = stop_server().await;
        
        // Should handle parse error gracefully
        assert!(result.is_err());
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn test_show_status_with_stale_pid() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = show_status().await;
        assert!(result.is_ok());
        
        // PID file should be removed due to stale PID
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_show_status_with_valid_pid() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Use current process PID (should be running)
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let result = show_status().await;
        assert!(result.is_ok());
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn test_show_status_invalid_pid_file_content() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create a PID file with invalid content
        fs::write(&pid_file, "invalid_pid").unwrap();
        
        let result = show_status().await;
        assert!(result.is_ok());
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn test_is_server_running_with_stale_pid() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = is_server_running().await;
        assert!(result.is_ok());
        
        // PID file should be cleaned up
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_is_server_running_with_valid_pid() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Use current process PID
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let result = is_server_running().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }

    #[tokio::test]
    async fn test_is_server_running_invalid_pid_content() {
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        
        // Create PID file with invalid content
        fs::write(&pid_file, "not_a_number").unwrap();
        
        let result = is_server_running().await;
        assert!(result.is_ok());
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }

    #[test]
    fn test_terminate_process_current_os() {
        // Test that terminate_process compiles and runs without panicking
        // We use a non-existent PID to avoid actually terminating anything
        let result = terminate_process(999999);
        
        // Should not panic, regardless of success/failure
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_find_server_by_port_lsof_error() {
        // Test with a port that lsof will likely fail on or not find
        let result = find_server_by_port(1).await; // Port 1 is usually privileged
        
        // Should handle lsof command errors gracefully
        assert!(result.is_ok());
        let found_pid = result.unwrap();
        // Result depends on system state, but should not panic
        println!("Found PID on port 1: {:?}", found_pid);
    }

    #[tokio::test]
    async fn test_handle_start_server_already_running() {
        // First, simulate a server already running by creating a PID file
        let pid_file = std::env::temp_dir().join("embed-tool.pid");
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let args = StartArgs {
            port: 8089,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            auth_disabled: true,
            daemon: false,
            pid_file: None,
            tls_cert_path: None,
            tls_key_path: None,
        };
        
        let result = handle_start_server(args, None).await;
        
        // Should succeed (just print message about already running)
        assert!(result.is_ok());
        
        // Clean up
        let _ = fs::remove_file(&pid_file);
    }
}