use aegis_controller::daemon::server::DaemonSupervisor;
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
    let log_dir = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".aegis"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    let _ = std::fs::create_dir_all(&log_dir);

    // Write to both stderr and the daemon log file so errors are always captured
    // regardless of whether running via launchd or manually in the background.
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("daemon.log"))
        .ok()
        .map(std::sync::Mutex::new);

    let file_layer = log_file.map(|f| {
        let writer = std::sync::Arc::new(f);
        fmt::layer().with_ansi(false).with_writer(move || {
            use std::io::Write;
            struct MutexWriter(std::sync::Arc<std::sync::Mutex<std::fs::File>>);
            impl Write for MutexWriter {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    self.0.lock().unwrap().write(buf)
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    self.0.lock().unwrap().flush()
                }
            }
            MutexWriter(std::sync::Arc::clone(&writer))
        })
    });

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(file_layer)
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

    // launchd runs with a minimal PATH; include Homebrew and common user-level bin dirs
    // so the daemon can find tmux, claude, git, code-review-graph, and other tools.
    let path_value = format!(
        "{home}/.local/bin:{home}/.claude/local:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    );

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.aegiscore.aegisd</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>run</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{path}</string>
        <key>HOME</key>
        <string>{home}</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{home}/.aegis/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{home}/.aegis/daemon.err.log</string>
</dict>
</plist>"#,
        bin = bin_path.display(),
        path = path_value,
        home = home,
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
