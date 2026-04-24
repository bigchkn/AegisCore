use std::path::PathBuf;
use crate::{client::DaemonClient, error::AegisCliError, output::Printer};

fn launchd_plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join("Library/LaunchAgents/com.aegiscore.aegisd.plist")
}

pub async fn start(printer: &Printer) -> Result<(), AegisCliError> {
    let plist = launchd_plist_path();
    if !plist.exists() {
        return Err(AegisCliError::InvalidArg(
            "launchd plist not installed. Run: aegis daemon install".into(),
        ));
    }
    let status = std::process::Command::new("launchctl")
        .args(["load", plist.to_str().unwrap_or("")])
        .status()?;
    if status.success() {
        printer.line("aegisd started.");
    } else {
        return Err(AegisCliError::InvalidArg("launchctl load failed.".into()));
    }
    Ok(())
}

pub async fn stop(printer: &Printer) -> Result<(), AegisCliError> {
    let plist = launchd_plist_path();
    let status = std::process::Command::new("launchctl")
        .args(["unload", plist.to_str().unwrap_or("")])
        .status()?;
    if status.success() {
        printer.line("aegisd stopped.");
    } else {
        return Err(AegisCliError::InvalidArg("launchctl unload failed.".into()));
    }
    Ok(())
}

pub async fn status(printer: &Printer, client: &DaemonClient) -> Result<(), AegisCliError> {
    let payload = client.request(None, "daemon.status", serde_json::json!({})).await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let ver = payload.get("version").and_then(|v| v.as_str()).unwrap_or("?");
    let uptime = payload.get("uptime_s").and_then(|v| v.as_u64()).unwrap_or(0);
    let projects = payload.get("projects").and_then(|v| v.as_u64()).unwrap_or(0);
    let socket = payload.get("socket_path").and_then(|v| v.as_str()).unwrap_or("?");

    let uptime_str = format_uptime(uptime);
    printer.kv(&[
        ("aegisd", &format!("v{ver} — running (uptime: {uptime_str})")),
        ("Projects:", &format!("{projects} registered")),
        ("Socket:", socket),
    ]);
    Ok(())
}

pub async fn install() -> Result<(), AegisCliError> {
    let status = std::process::Command::new("aegisd").arg("install").status()?;
    if !status.success() {
        return Err(AegisCliError::InvalidArg("aegisd install failed.".into()));
    }
    Ok(())
}

pub async fn uninstall() -> Result<(), AegisCliError> {
    let status = std::process::Command::new("aegisd").arg("uninstall").status()?;
    if !status.success() {
        return Err(AegisCliError::InvalidArg("aegisd uninstall failed.".into()));
    }
    Ok(())
}

pub async fn projects(printer: &Printer, client: &DaemonClient) -> Result<(), AegisCliError> {
    let payload = client.request(None, "projects.list", serde_json::json!({})).await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let projects = match payload.as_array() {
        Some(arr) => arr.clone(),
        None => payload
            .get("projects")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
    };

    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|p| {
            vec![
                p.get("root_path")
                    .or_else(|| p.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                p.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                p.get("last_seen")
                    .and_then(|v| v.as_str())
                    .unwrap_or("—")
                    .to_string(),
            ]
        })
        .collect();

    printer.table(&["PATH", "STATUS", "LAST ACTIVE"], rows);
    Ok(())
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
