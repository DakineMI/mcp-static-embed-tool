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

async fn handle_start_server(
    args: StartArgs,
    _config_path: Option<PathBuf>,
) -> AnyhowResult<()> {
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