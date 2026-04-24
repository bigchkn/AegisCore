use aegis_controller::daemon::{UdsRequest, UdsResponse};
use aegis_core::{AegisError, Result};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "aegis")]
#[command(about = "AegisCore CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to the Unix Domain Socket
    #[arg(long, default_value = "/tmp/aegis.sock", global = true)]
    uds_path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        subcommand: DaemonCommands,
    },
    /// Project management
    Projects {
        #[command(subcommand)]
        subcommand: ProjectCommands,
    },
    /// Agent management (stub)
    Agents {
        #[arg(short, long)]
        list: bool,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon (via aegisd)
    Start,
    /// Stop the daemon (via launchctl)
    Stop,
    /// Install launchd plist
    Install,
    /// Uninstall launchd plist
    Uninstall,
}

#[derive(Subcommand)]
enum ProjectCommands {
    /// List all registered projects
    List,
    /// Register the current directory as an Aegis project
    Init,
    /// Unregister a project by ID
    Remove { id: Uuid },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon { subcommand } => handle_daemon(subcommand).await,
        Commands::Projects { subcommand } => handle_projects(subcommand, cli.uds_path).await,
        Commands::Agents { list: _ } => {
            println!("Agent management not yet implemented (M10/M12).");
            Ok(())
        }
    }
}

async fn handle_daemon(cmd: DaemonCommands) -> Result<()> {
    match cmd {
        DaemonCommands::Start => {
            println!("Starting aegisd...");
            let home = std::env::var("HOME").unwrap();
            let plist_path = format!("{}/Library/LaunchAgents/com.aegiscore.aegisd.plist", home);
            std::process::Command::new("launchctl")
                .arg("load")
                .arg(plist_path)
                .status()
                .map_err(|e| AegisError::Unexpected(Box::new(e)))?;
        }
        DaemonCommands::Stop => {
            println!("Stopping aegisd...");
            let home = std::env::var("HOME").unwrap();
            let plist_path = format!("{}/Library/LaunchAgents/com.aegiscore.aegisd.plist", home);
            std::process::Command::new("launchctl")
                .arg("unload")
                .arg(plist_path)
                .status()
                .map_err(|e| AegisError::Unexpected(Box::new(e)))?;
        }
        DaemonCommands::Install => {
            std::process::Command::new("aegisd")
                .arg("install")
                .status()
                .map_err(|e| AegisError::Unexpected(Box::new(e)))?;
        }
        DaemonCommands::Uninstall => {
            std::process::Command::new("aegisd")
                .arg("uninstall")
                .status()
                .map_err(|e| AegisError::Unexpected(Box::new(e)))?;
        }
    }
    Ok(())
}

async fn handle_projects(cmd: ProjectCommands, uds_path: PathBuf) -> Result<()> {
    match cmd {
        ProjectCommands::List => {
            let response =
                send_uds_command(uds_path, "list_projects", serde_json::json!({})).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&response.payload).unwrap()
            );
        }
        ProjectCommands::Init => {
            let current_dir = std::env::current_dir().map_err(|e| AegisError::StorageIo {
                path: ".".into(),
                source: e,
            })?;
            let response = send_uds_command(
                uds_path,
                "register_project",
                serde_json::json!({ "path": current_dir }),
            )
            .await?;
            println!("Project registered: {}", response.status);
        }
        ProjectCommands::Remove { id } => {
            let response = send_uds_command(
                uds_path,
                "unregister_project",
                serde_json::json!({ "id": id }),
            )
            .await?;
            println!("Project unregistered: {}", response.status);
        }
    }
    Ok(())
}

async fn send_uds_command(
    path: PathBuf,
    command: &str,
    params: serde_json::Value,
) -> Result<UdsResponse> {
    let stream = UnixStream::connect(path.clone())
        .await
        .map_err(|_| AegisError::DaemonNotRunning { socket_path: path })?;

    let mut lines = Framed::new(stream, LinesCodec::new());

    let request = UdsRequest {
        id: Uuid::new_v4(),
        project_path: None,
        command: command.to_string(),
        params,
    };

    let request_json = serde_json::to_string(&request).map_err(|e| AegisError::IpcProtocol {
        reason: e.to_string(),
    })?;

    lines
        .send(request_json)
        .await
        .map_err(|e| AegisError::IpcProtocol {
            reason: e.to_string(),
        })?;

    if let Some(result) = lines.next().await {
        let line: String = result.map_err(|e| AegisError::IpcProtocol {
            reason: e.to_string(),
        })?;
        let response: UdsResponse =
            serde_json::from_str(&line).map_err(|e| AegisError::IpcProtocol {
                reason: e.to_string(),
            })?;
        Ok(response)
    } else {
        Err(AegisError::IpcProtocol {
            reason: "No response from daemon".to_string(),
        })
    }
}
