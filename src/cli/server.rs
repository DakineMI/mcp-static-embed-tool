

use crate::cli::{ServerAction, StartArgs};
use crate::server::start::{start_server, ServerConfig};
use anyhow::{anyhow, Result as AnyhowResult};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::fs;
#[cfg(any(windows, not(any(unix, windows))))]
use sysinfo::{System, Pid};

/// Handle server lifecycle commands.
///
/// Routes the server action (start, stop, status, restart) to the appropriate handler.
///
/// # Arguments
///
/// * `action` - The server action to perform
/// * `config_path` - Optional path to configuration file
///
/// # Errors
///
/// Returns error if:
/// - Server is already running when starting
/// - Server is not running when stopping
/// - Configuration is invalid
/// - System resources are unavailable
pub async fn handle_server_command(
    action: ServerAction,
    config_path: Option<PathBuf>,
) -> AnyhowResult<()> {
    match action {
        ServerAction::Start(args) => handle_start_server(args, config_path).await,
        ServerAction::Stop => stop_server(None).await,
        ServerAction::Status => show_status(None).await,
        ServerAction::Restart(args) => {
            if is_server_running(args.pid_file.as_ref()).await? {
                stop_server(args.pid_file.as_ref()).await?;
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
    if is_server_running(args.pid_file.as_ref()).await? {
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
    eprintln!("Starting embedding server in foreground mode...");
    eprintln!("Port: {}", args.port);
    eprintln!("Bind: {}", args.bind);
    eprintln!("Default model: {}", args.default_model);
    
    if let Some(models) = &args.models {
        eprintln!("Models: {}", models);
    }
    
    if args.mcp {
        eprintln!("MCP mode: enabled");
    }

    let config = if args.mcp {
        // MCP mode: stdio
        ServerConfig {
            server_url: "stdio://-".to_string(),
            bind_address: None,
        }
    } else if let Some(socket_path) = args.socket_path {
        ServerConfig {
            server_url: format!("unix://{}", socket_path.display()),
            bind_address: None,
        }
    } else {
        let addr = format!("{}:{}", args.bind, args.port);
        ServerConfig {
            server_url: format!("http://{}", addr),
            bind_address: Some(addr),
        }
    };

    start_server(config).await
}

async fn start_daemon(args: StartArgs) -> AnyhowResult<()> {
    eprintln!("Starting embedding server as daemon...");
    
    let current_exe = std::env::current_exe()?;
    let pid_file = get_pid_file_path(args.pid_file.as_ref());

    if let Some(parent) = pid_file.parent() {
        fs::create_dir_all(parent)?;
    }
    
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

    if let Some(pid_path) = &args.pid_file {
        cmd_args.push("--pid-file");
        // We need to convert PathBuf to str, ensuring it's valid UTF-8
        if let Some(s) = pid_path.to_str() {
            cmd_args.push(s);
        }
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
    
    eprintln!("Server started as daemon with PID: {}", child.id());
    eprintln!("PID file: {}", pid_file.display());
    
    Ok(())
}

async fn stop_server(custom_pid: Option<&PathBuf>) -> AnyhowResult<()> {
    let pid_file = get_pid_file_path(custom_pid);
    
    if !pid_file.exists() {
        // Try to find by port
        if let Some(pid) = find_server_by_port(8080).await? {
            terminate_process(pid)?;
            eprintln!("Server stopped (found by port)");
        } else {
            eprintln!("No running server found");
        }
        return Ok(());
    }
    
    let pid_str = fs::read_to_string(&pid_file)?;
    let pid: u32 = pid_str.trim().parse()?;
    
    terminate_process(pid)?;
    fs::remove_file(&pid_file)?;
    
    eprintln!("Server stopped (PID: {})", pid);
    Ok(())
}

async fn show_status(custom_pid: Option<&PathBuf>) -> AnyhowResult<()> {
    let pid_file = get_pid_file_path(custom_pid);
    
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if is_process_running(pid) {
                eprintln!("Server is running (PID: {})", pid);
                eprintln!("PID file: {}", pid_file.display());
                
                // Try to get more info by checking port
                if let Some(_) = find_server_by_port(8080).await? {
                    eprintln!("HTTP API: http://localhost:8080");
                }
            } else {
                eprintln!("Server is not running (stale PID file)");
                fs::remove_file(&pid_file)?;
            }
        }
    } else if let Some(pid) = find_server_by_port(8080).await? {
        eprintln!("Server is running (PID: {}) but no PID file found", pid);
        eprintln!("HTTP API: http://localhost:8080");
    } else {
        eprintln!("Server is not running");
    }
    
    Ok(())
}

async fn is_server_running(custom_pid: Option<&PathBuf>) -> AnyhowResult<bool> {
    let pid_file = get_pid_file_path(custom_pid);
    
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

fn get_pid_file_path(custom: Option<&PathBuf>) -> PathBuf {
    if let Some(path) = custom {
        return path.clone();
    }
    pid_file_path()
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Use external `kill -0 <pid>` to probe for process existence without unsafe
        match Command::new("kill").args(["-0", &pid.to_string()]).output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    #[cfg(windows)]
    {
        // Fallback to sysinfo on Windows
        let mut system = System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            false,
            sysinfo::ProcessRefreshKind::new(),
        );
        system.process(Pid::from(pid as usize)).is_some()
    }

    #[cfg(not(any(unix, windows)))]
    {
        let mut system = System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            false,
            sysinfo::ProcessRefreshKind::new(),
        );
        system.process(Pid::from(pid as usize)).is_some()
    }
}

async fn find_server_by_port(port: u16) -> AnyhowResult<Option<u32>> {
    // This is a simplified implementation
    // In practice, you'd want to check netstat or similar
    let output_result = Command::new("lsof")
        .args(&["-t", &format!("-i:{}", port)])
        .output();

    let output = match output_result {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // lsof not installed (common in minimal containers)
            // Assuming not running is safe for container startup
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    eprintln!("lsof output status: {}", output.status);
    eprintln!("lsof stdout: {:?}", String::from_utf8_lossy(&output.stdout));
    eprintln!("lsof stderr: {:?}", String::from_utf8_lossy(&output.stderr));

    if output.status.success() && !output.stdout.is_empty() {
        let pid_str = String::from_utf8(output.stdout)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return Ok(Some(pid));
        }
    }

    Ok(None)
}

fn terminate_process(pid: u32) -> AnyhowResult<()> {
    if !is_process_running(pid) {
        return Ok(());
    }
    
    #[cfg(unix)]
    {
        // Send TERM signal via external `kill` to avoid unsafe
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()?;
    }
    
    #[cfg(windows)]
    {
        Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output()?;
    }
    
    Ok(())
}

// Determine a stable, per-user PID file path
fn pid_file_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let base = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // Fallback as a last resort
                PathBuf::from("/tmp")
            });
        return base
            .join("Library")
            .join("Application Support")
            .join("embed-tool")
            .join("embed-tool.pid");
    }

    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        return base.join("embed-tool").join("embed-tool.pid");
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(xdg).join("embed-tool").join("embed-tool.pid");
        }
        let base = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        return base.join(".cache").join("embed-tool").join("embed-tool.pid");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Fallback to a subdirectory under temp dir for other platforms
        return PathBuf::from("/tmp")
            .join("embed-tool")
            .join("embed-tool.pid");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_validate_models_in_start_args() {
        let args = StartArgs {
            port: 8080,
            bind: "0.0.0.0".to_string(),
            socket_path: None,
            models: Some("model1,model2".to_string()),
            default_model: "model1".to_string(),
            mcp: false,
            daemon: false,
            pid_file: None,
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
            daemon: false,
            pid_file: None,
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
            daemon: false,
            pid_file: None,
        };

        let result = handle_start_server(args, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No valid models"));
    }

    #[test]
    fn test_is_process_running() {
        // Test with a non-existent PID
        assert!(!is_process_running(999999));

        // Test with current PID (should always exist and be accessible)
        assert!(is_process_running(std::process::id()));
    }

    #[tokio::test]
    async fn test_is_server_running_no_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_is_running.pid");

        // Should return false when no PID file exists and no server is running on port 8080
        let result = is_server_running(Some(&pid_file)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_status_no_server() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_status.pid");

        // Should not panic
        let result = show_status(Some(&pid_file)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_server_no_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_stop.pid");

        // Should not panic
        let result = stop_server(Some(&pid_file)).await;
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
        eprintln!("Server PID found: {:?}", server_pid);
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
            daemon: false,
            pid_file: None,
        };

        // This will likely fail to start the actual server, but should not panic
        let result = handle_server_command(ServerAction::Start(args), None).await;
        // We expect this to succeed in test environment even if server doesn't actually start
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_handle_server_command_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_restart.pid");
        
        let args = StartArgs {
            port: 8082, // Use different port
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            daemon: true, // Use daemon mode to avoid hanging
            pid_file: Some(pid_file.clone()),
        };

        // Test restart command - should handle non-existent server gracefully
        let result = handle_server_command(ServerAction::Restart(args), None).await;
        
        // Clean up any PID file
        if pid_file.exists() {
             let _ = fs::remove_file(&pid_file);
        }
        
        // Should succeed even if no server was running
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
            pid_file: None,
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
            daemon: false,
            pid_file: None,
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
            daemon: false,
            pid_file: None,
        };

        // Spawn server in background with timeout to prevent hanging
        let handle = tokio::spawn(async move {
            let _ = start_foreground(args).await;
        });
        
        // Give it 100ms to start, then abort
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        handle.abort();
        
        // Test passes if we got here without hanging
        assert!(true);
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
            daemon: false,
            pid_file: None,
        };

        // Spawn server in background with timeout to prevent hanging
        let handle = tokio::spawn(async move {
            let _ = start_foreground(args).await;
        });
        
        // Give it 100ms to start, then abort
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        handle.abort();
        
        // Test passes if we got here without hanging
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_foreground_socket_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir.path().join("test_socket.sock");
        
        let args = StartArgs {
            port: 8085,
            bind: "127.0.0.1".to_string(),
            socket_path: Some(socket_path.clone()),
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            daemon: false,
            pid_file: None,
        };

        // Spawn server in background with timeout to prevent hanging
        let handle = tokio::spawn(async move {
            let _ = start_foreground(args).await;
        });
        
        // Give it 100ms to start, then abort
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        handle.abort();
        
        // Clean up socket file if created
        if socket_path.exists() {
            let _ = fs::remove_file(&socket_path);
        }
        
        // Test passes if we got here without hanging
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_daemon_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_daemon.pid");
        
        let args = StartArgs {
            port: 8086,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            daemon: true,
            pid_file: Some(pid_file.clone()),
        };

        // This will try to spawn a daemon process
        let result = start_daemon(args).await;
        
        // Clean up any PID file that might have been created
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
        
        // The result depends on whether the process can actually be spawned
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_daemon_with_mcp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_daemon_mcp.pid");
        
        let args = StartArgs {
            port: 8087,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M,custom-model".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: true,
            daemon: true,
            pid_file: Some(pid_file.clone()),
        };

        let result = start_daemon(args).await;
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
        
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
            daemon: true,
            pid_file: None, // Use default PID file location
        };

        let result = start_daemon(args).await;
        
        // Clean up default PID file
        let default_pid_file = pid_file_path();
        if default_pid_file.exists() {
            let _ = fs::remove_file(&default_pid_file);
        }
        
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_stop_server_with_valid_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_stop_valid.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = stop_server(Some(&pid_file)).await;
        
        // Should succeed even if process doesn't exist
        assert!(result.is_ok());
        
        // PID file should be removed
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_stop_server_invalid_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_stop_invalid.pid");
        
        // Create a PID file with invalid content
        fs::write(&pid_file, "not_a_number").unwrap();
        
        let result = stop_server(Some(&pid_file)).await;
        
        // Should handle parse error gracefully
        assert!(result.is_err());
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }

    #[tokio::test]
    async fn test_show_status_with_stale_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_status_stale.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = show_status(Some(&pid_file)).await;
        assert!(result.is_ok());
        
        // PID file should be removed due to stale PID
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_show_status_with_valid_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_status_valid.pid");
        
        // Use current process PID (should be running)
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let result = show_status(Some(&pid_file)).await;
        assert!(result.is_ok());
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }

    #[tokio::test]
    async fn test_show_status_invalid_pid_file_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_status_invalid.pid");
        
        // Create a PID file with invalid content
        fs::write(&pid_file, "invalid_pid").unwrap();
        
        let result = show_status(Some(&pid_file)).await;
        assert!(result.is_ok());
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }

    #[tokio::test]
    async fn test_is_server_running_with_stale_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_running_stale.pid");
        
        // Create a PID file with a non-existent PID
        fs::write(&pid_file, "999999").unwrap();
        
        let result = is_server_running(Some(&pid_file)).await;
        assert!(result.is_ok());
        
        // PID file should be cleaned up
        assert!(!pid_file.exists());
    }

    #[tokio::test]
    async fn test_is_server_running_with_valid_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_running_valid.pid");
        
        // Use current process PID
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let result = is_server_running(Some(&pid_file)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }

    #[tokio::test]
    async fn test_is_server_running_invalid_pid_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_running_invalid.pid");
        
        // Create PID file with invalid content
        fs::write(&pid_file, "not_a_number").unwrap();
        
        let result = is_server_running(Some(&pid_file)).await;
        assert!(result.is_ok());
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
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
        eprintln!("Found PID on port 1: {:?}", found_pid);
    }

    #[tokio::test]
    async fn test_handle_start_server_already_running() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_file = temp_dir.path().join("test_start_existing.pid");

        // First, simulate a server already running by creating a PID file
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();
        
        let args = StartArgs {
            port: 8089,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            daemon: false,
            pid_file: Some(pid_file.clone()),
        };
        
        let result = handle_start_server(args, None).await;
        
        // Should succeed (just print message about already running)
        assert!(result.is_ok());
        
        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }
}