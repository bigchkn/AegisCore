use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};
use std::path::Path;
use uuid::Uuid;

pub async fn start(
    role: Option<&str>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let params = match role {
        Some(r) => serde_json::json!({ "role": r }),
        None => serde_json::json!({}),
    };
    let payload = client
        .request(Some(&anchor.project_root), "session.start", params)
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    if let Some(agents) = payload.as_array() {
        for a in agents {
            let id = a.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
            let role = a.get("role").and_then(|v| v.as_str()).unwrap_or("?");
            printer.line(&format!("Bastion started: {role} ({id})"));
        }
    } else {
        printer.line("Session started.");
    }
    Ok(())
}

pub async fn stop(
    force: bool,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "session.stop",
            serde_json::json!({ "force": force }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    printer.line(if force {
        "All agents terminated."
    } else {
        "Session stopped. Agent worktrees preserved."
    });
    Ok(())
}

/// `aegis attach [<agent_id>]` — local tmux passthrough, no UDS call.
pub fn attach(agent_id: Option<Uuid>, anchor: &ProjectAnchor) -> Result<(), AegisCliError> {
    let session_name =
        read_session_name(&anchor.project_root).unwrap_or_else(|| "aegis".to_string());

    if let Some(id) = agent_id {
        // Try to find pane target from registry file
        let pane =
            find_agent_pane(&anchor.aegis_dir, id).unwrap_or_else(|| format!("{session_name}:0"));
        exec_tmux(&["select-window", "-t", &pane])?;
    } else {
        exec_tmux(&["attach-session", "-t", &session_name])?;
    }
    Ok(())
}

fn read_session_name(project_root: &Path) -> Option<String> {
    use aegis_core::config::EffectiveConfig;
    let global = EffectiveConfig::load_global().unwrap_or_default();
    let project = EffectiveConfig::load_project(project_root).unwrap_or_default();
    EffectiveConfig::resolve(&global, &project)
        .ok()
        .map(|c| c.global.tmux_session_name)
}

fn find_agent_pane(aegis_dir: &Path, agent_id: Uuid) -> Option<String> {
    let registry_path = aegis_dir.join("state/registry.json");
    let data = std::fs::read_to_string(registry_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    let agents = v.get("agents")?.as_array()?;
    let id_str = agent_id.to_string();
    for a in agents {
        if a.get("agent_id").and_then(|v| v.as_str()) == Some(&id_str) {
            let session = a
                .get("tmux_session")
                .and_then(|v| v.as_str())
                .unwrap_or("aegis");
            let window = a.get("tmux_window").and_then(|v| v.as_u64()).unwrap_or(0);
            let pane = a.get("tmux_pane").and_then(|v| v.as_str()).unwrap_or("%0");
            return Some(format!("{session}:{window}.{pane}"));
        }
    }
    None
}

fn exec_tmux(args: &[&str]) -> Result<(), AegisCliError> {
    let status = std::process::Command::new("tmux").args(args).status()?;
    if !status.success() {
        return Err(AegisCliError::InvalidArg(format!(
            "tmux {} failed",
            args.first().unwrap_or(&"")
        )));
    }
    Ok(())
}
