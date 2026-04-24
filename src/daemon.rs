use aegis_controller::daemon::DaemonSupervisor;
use aegis_core::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "aegisd")]
#[command(about = "AegisCore Global Daemon", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon in the foreground
    Run {
        /// Path to the Unix Domain Socket
        #[arg(long, default_value = "/tmp/aegis.sock")]
        uds_path: PathBuf,

        /// Port for the HTTP server
        #[arg(long, default_value_t = 7437)]
        http_port: u16,
    },
    /// Install the launchd plist for the current user
    Install,
    /// Uninstall the launchd plist
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            uds_path,
            http_port,
        } => {
            let supervisor = DaemonSupervisor::new(uds_path, http_port);
            supervisor.run().await?;
        }
        Commands::Install => {
            install_launchd()?;
        }
        Commands::Uninstall => {
            uninstall_launchd()?;
        }
    }

    Ok(())
}

fn install_launchd() -> Result<()> {
    let home = std::env::var("HOME").unwrap();
    let plist_path = PathBuf::from(&home).join("Library/LaunchAgents/com.aegiscore.aegisd.plist");
    let bin_path =
        std::env::current_exe().map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))?;

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.aegiscore.aegisd</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{}/.aegis/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{}/.aegis/daemon.err.log</string>
</dict>
</plist>"#,
        bin_path.display(),
        home,
        home
    );

    std::fs::create_dir_all(plist_path.parent().unwrap()).map_err(|e| {
        aegis_core::AegisError::StorageIo {
            path: plist_path.clone(),
            source: e,
        }
    })?;

    std::fs::write(&plist_path, plist_content).map_err(|e| aegis_core::AegisError::StorageIo {
        path: plist_path.clone(),
        source: e,
    })?;

    println!("Installed launchd plist to {}", plist_path.display());
    println!("To start now: launchctl load {}", plist_path.display());

    Ok(())
}

fn uninstall_launchd() -> Result<()> {
    let home = std::env::var("HOME").unwrap();
    let plist_path = PathBuf::from(&home).join("Library/LaunchAgents/com.aegiscore.aegisd.plist");

    if plist_path.exists() {
        // Try to unload first
        let _ = std::process::Command::new("launchctl")
            .arg("unload")
            .arg(&plist_path)
            .status();

        std::fs::remove_file(&plist_path).map_err(|e| aegis_core::AegisError::StorageIo {
            path: plist_path.clone(),
            source: e,
        })?;
        println!("Uninstalled launchd plist from {}", plist_path.display());
    } else {
        println!("No launchd plist found at {}", plist_path.display());
    }

    Ok(())
}
