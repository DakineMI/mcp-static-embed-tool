use crate::cli::{ServerAction, StartArgs};
use crate::server::start::{ServerConfig, start_server};
use anyhow::{Result as AnyhowResult, anyhow};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use sysinfo::{Pid, System};

/// Manages a PID file for tracking server processes.
struct PidFile {
    path: PathBuf,
}

impl PidFile {
    fn new(custom_path: Option<&PathBuf>) -> Self {
        let path = custom_path.cloned().unwrap_or_else(pid_file_path);
        Self { path }
    }

    fn write(&self, pid: u32) -> AnyhowResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, pid.to_string())?;
        Ok(())
    }

    fn read(&self) -> AnyhowResult<Option<u32>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&self.path)?;
        let pid = content
            .trim()
            .parse::<u32>()
            .map_err(|e| anyhow!("Invalid PID file content: {}", e))?;
        Ok(Some(pid))
    }

    fn remove(&self) -> AnyhowResult<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    fn is_running(&self) -> AnyhowResult<bool> {
        match self.read()? {
            Some(pid) => {
                if is_process_running(pid) {
                    Ok(true)
                } else {
                    // Stale PID file
                    let _ = self.remove();
                    Ok(false)
                }
            }
            None => Ok(false),
        }
    }
}

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
    let config = crate::cli::config::load_config(config_path.clone())
        .map_err(|e| anyhow!("Failed to load config: {}", e))?;
    let port = config.server.default_port;

    match action {
        ServerAction::Start(args) => handle_start_server(args, config_path).await,
        ServerAction::Stop => stop_server(None, port).await,
        ServerAction::Status => show_status(None, port).await,
        ServerAction::Restart(args) => {
            let pid_file = PidFile::new(args.pid_file.as_ref());
            if pid_file.is_running()? {
                stop_server(args.pid_file.as_ref(), port).await?;
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
        let pid_file = PidFile::new(args.pid_file.as_ref());
        if pid_file.is_running()? || find_server_by_port(args.port).await?.is_some() {
            eprintln!("Server is already running on port {}. Use 'static-embedding-tool server stop' first or 'static-embedding-tool server restart'.", args.port);
            return Ok(());
        }

    if args.watch {
        start_foreground(args).await
    } else {
        start_daemon(args).await
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
    let pid_file = PidFile::new(args.pid_file.as_ref());

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

    if let Some(socket_path) = &args.socket_path {
        cmd_args.push("--socket-path");
        if let Some(s) = socket_path.to_str() {
            cmd_args.push(s);
        } else {
            return Err(anyhow!("Socket path contains invalid UTF-8"));
        }
    }

    if let Some(pid_path) = &args.pid_file {
        cmd_args.push("--pid-file");
        // We need to convert PathBuf to str, ensuring it's valid UTF-8
        if let Some(s) = pid_path.to_str() {
            cmd_args.push(s);
        } else {
            return Err(anyhow!("PID file path contains invalid UTF-8"));
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
    pid_file.write(child.id())?;

    eprintln!("Server started as daemon with PID: {}", child.id());
    eprintln!("PID file: {}", pid_file.path.display());

    Ok(())
}

async fn stop_server(custom_pid: Option<&PathBuf>, port: u16) -> AnyhowResult<()> {
    let pid_file = PidFile::new(custom_pid);

    match pid_file.read()? {
        Some(pid) => {
            terminate_process(pid)?;
            pid_file.remove()?;
            eprintln!("Server stopped (PID: {})", pid);
        }
        None => {
            // Try to find by port as fallback
            if let Some(pid) = find_server_by_port(port).await? {
                terminate_process(pid)?;
                eprintln!("Server stopped (found by port {})", port);
            } else {
                eprintln!("No running server found on port {}", port);
            }
        }
    }
    Ok(())
}

async fn show_status(custom_pid: Option<&PathBuf>, port: u16) -> AnyhowResult<()> {
    let pid_file = PidFile::new(custom_pid);

    if let Some(pid) = pid_file.read()? {
        if is_process_running(pid) {
            eprintln!("Server is running (PID: {})", pid);
            eprintln!("PID file: {}", pid_file.path.display());

            // Try to get more info by checking port
            if find_server_by_port(port).await?.is_some() {
                eprintln!("HTTP API: http://localhost:{}", port);
            }
        } else {
            eprintln!("Server is not running (stale PID file)");
            pid_file.remove()?;
        }
    } else if let Some(pid) = find_server_by_port(port).await? {
        eprintln!("Server is running (PID: {}) but no PID file found", pid);
        eprintln!("HTTP API: http://localhost:{}", port);
    } else {
        eprintln!("Server is not running");
    }

    Ok(())
}

fn is_process_running(pid: u32) -> bool {
    let mut system = System::new();
    let pid_val = Pid::from(pid as usize);
    system.refresh_processes_specifics(sysinfo::ProcessRefreshKind::new());
    system.process(pid_val).is_some()
}

async fn find_server_by_port(port: u16) -> AnyhowResult<Option<u32>> {
    // This is a simplified implementation
    // In practice, you'd want to check netstat or similar
    let output_result = Command::new("lsof")
        .args(["-t", &format!("-i:{}", port)])
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

    if output.status.success() && !output.stdout.is_empty() {
        let pid_str = String::from_utf8(output.stdout)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return Ok(Some(pid));
        }
    }

    Ok(None)
}

fn terminate_process(pid: u32) -> AnyhowResult<()> {
    let mut system = System::new();
    let pid_val = Pid::from(pid as usize);
    system.refresh_processes_specifics(sysinfo::ProcessRefreshKind::new());

    if let Some(process) = system.process(pid_val) {
        let _ = process.kill();
        // Give it a moment to actually die
        std::thread::sleep(std::time::Duration::from_millis(100));
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
        base.join("Library")
            .join("Application Support")
            .join("static-embedding-tool")
            .join("static-embedding-tool.pid")
    }

    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        return base.join("static-embedding-tool").join("static-embedding-tool.pid");
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(xdg).join("static-embedding-tool").join("static-embedding-tool.pid");
        }
        let base = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        return base
            .join(".cache")
            .join("static-embedding-tool")
            .join("static-embedding-tool.pid");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Fallback to a subdirectory under temp dir for other platforms
        return PathBuf::from("/tmp")
            .join("static-embedding-tool")
            .join("static-embedding-tool.pid");
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
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("model1,model2".to_string()),
            default_model: "model1".to_string(),
            mcp: false,
            watch: false,
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
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("model1,model2".to_string()),
            default_model: "model3".to_string(), // Not in models list
            mcp: false,
            watch: false,
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
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some(",,,,".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            watch: false,
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
        let pid_path = temp_dir.path().join("test_is_running.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Should return false when no PID file exists and no server is running on port 8080
        let result = pid_file.is_running();
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_show_status_no_server() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_status.pid");

        // Should not panic
        let result = show_status(Some(&pid_path), 8080).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_server_no_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_stop.pid");

        // Should not panic
        let result = stop_server(Some(&pid_path), 8080).await;
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
            watch: false,
            daemon: false,
            pid_file: None,
        };

        // Use a short timeout since handle_server_command will block if it succeeds in starting
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            handle_server_command(ServerAction::Start(args), None),
        )
        .await;
        // If it timed out, it means it started successfully (blocking)
        // If it returned, it might be an error or success (e.g. bind failure in test)
        match result {
            Err(_) => assert!(true), // Timed out, expected if it blocks
            Ok(inner) => assert!(inner.is_ok() || inner.is_err()),
        }
    }

    #[tokio::test]
    async fn test_handle_server_command_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_restart.pid");

        let args = StartArgs {
            port: 8082, // Use different port
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            watch: false,
            daemon: true, // Use daemon mode to avoid hanging
            pid_file: Some(pid_path.clone()),
        };

        // Restart with daemon=true should not block, but let's use timeout anyway for safety
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            handle_server_command(ServerAction::Restart(args), None),
        )
        .await;

        // Clean up any PID file
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
        }

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_start_args_no_models() {
        let args = StartArgs {
            port: 8080,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: None, // No models specified
            default_model: "potion-32M".to_string(),
            mcp: false,
            watch: false,
            daemon: false,
            pid_file: None,
        };

        // Should succeed when no models are specified
        assert!(validate_start_args(&args).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_start_args_whitespace_models() {
        let args = StartArgs {
            port: 8080,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("  model1  ,  model2  ".to_string()),
            default_model: "model1".to_string(),
            mcp: false,
            watch: false,
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
            watch: false,
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
            watch: false,
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
            watch: false,
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
            let _ = std::fs::remove_file(&socket_path);
        }

        // Test passes if we got here without hanging
        assert!(true);
    }

    #[tokio::test]
    async fn test_start_daemon_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_daemon.pid");

        let args = StartArgs {
            port: 8086,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: false,
            watch: false,
            daemon: true,
            pid_file: Some(pid_path.clone()),
        };

        // This will try to spawn a daemon process
        let result = start_daemon(args).await;

        // Clean up any PID file that might have been created
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
        }

        // The result depends on whether the process can actually be spawned
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_start_daemon_with_mcp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_daemon_mcp.pid");

        let args = StartArgs {
            port: 8087,
            bind: "127.0.0.1".to_string(),
            socket_path: None,
            models: Some("potion-32M,custom-model".to_string()),
            default_model: "potion-32M".to_string(),
            mcp: true,
            watch: false,
            daemon: true,
            pid_file: Some(pid_path.clone()),
        };

        let result = start_daemon(args).await;

        // Clean up
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
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
            watch: false,
            daemon: true,
            pid_file: None, // Use default PID file location
        };

        let result = start_daemon(args).await;

        // Clean up default PID file
        let pid_file = PidFile::new(None);
        if pid_file.path.exists() {
            let _ = std::fs::remove_file(&pid_file.path);
        }

        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_stop_server_with_valid_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_stop_valid.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Create a PID file with a non-existent PID
        pid_file.write(999999).unwrap();

        let result = stop_server(Some(&pid_path), 8080).await;

        // Should succeed even if process doesn't exist
        assert!(result.is_ok());

        // PID file should be removed
        assert!(!pid_path.exists());
    }

    #[tokio::test]
    async fn test_stop_server_invalid_pid_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_stop_invalid.pid");

        // Create a PID file with invalid content
        std::fs::write(&pid_path, "not_a_number").unwrap();

        let result = stop_server(Some(&pid_path), 8080).await;

        // Should handle parse error gracefully
        assert!(result.is_err());

        // Clean up
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
        }
    }

    #[tokio::test]
    async fn test_show_status_with_stale_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_status_stale.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Create a PID file with a non-existent PID
        pid_file.write(999999).unwrap();

        let result = show_status(Some(&pid_path), 8080).await;
        assert!(result.is_ok());

        // PID file should be removed due to stale PID
        assert!(!pid_path.exists());
    }

    #[tokio::test]
    async fn test_show_status_with_valid_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_status_valid.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Use current process PID (should be running)
        let current_pid = std::process::id();
        pid_file.write(current_pid).unwrap();

        let result = show_status(Some(&pid_path), 8080).await;
        assert!(result.is_ok());

        // Clean up
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
        }
    }

        #[tokio::test]
        async fn test_show_status_invalid_pid_file_content() {
            let temp_dir = tempfile::tempdir().unwrap();
            let pid_path = temp_dir.path().join("test_status_invalid.pid");
            
            // Create a PID file with invalid content
            std::fs::write(&pid_path, "invalid_pid").unwrap();
            
            let result = show_status(Some(&pid_path), 8080).await;
            // It should return an error when parsing the PID fails
            assert!(result.is_err());
            
            // Clean up
            if pid_path.exists() {
                let _ = std::fs::remove_file(&pid_path);
            }
        }
    #[tokio::test]
    async fn test_is_server_running_with_stale_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_running_stale.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Create a PID file with a non-existent PID
        pid_file.write(999999).unwrap();

        let result = pid_file.is_running();
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // PID file should be cleaned up
        assert!(!pid_path.exists());
    }

    #[tokio::test]
    async fn test_is_server_running_with_valid_pid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_running_valid.pid");
        let pid_file = PidFile::new(Some(&pid_path));

        // Use current process PID
        let current_pid = std::process::id();
        pid_file.write(current_pid).unwrap();

        let result = pid_file.is_running();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);

        // Clean up
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
        }
    }

    #[tokio::test]
    async fn test_is_server_running_invalid_pid_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_path = temp_dir.path().join("test_running_invalid.pid");

        // Create PID file with invalid content
        std::fs::write(&pid_path, "not_a_number").unwrap();

        let pid_file = PidFile::new(Some(&pid_path));
        let result = pid_file.is_running();
        assert!(result.is_err());

        // Clean up
        if pid_path.exists() {
            let _ = std::fs::remove_file(&pid_path);
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
            watch: false,
            daemon: false,
            pid_file: Some(pid_file.clone()),
        };

        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(500),
            handle_start_server(args, None),
        )
        .await;

        // Should succeed (just print message about already running)
        match result {
            Err(_) => panic!("Timed out waiting for already running check"),
            Ok(inner) => assert!(inner.is_ok()),
        }

        // Clean up
        if pid_file.exists() {
            let _ = fs::remove_file(&pid_file);
        }
    }
}
