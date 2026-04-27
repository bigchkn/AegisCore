use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};
use uuid::Uuid;

pub async fn list(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "agents.list",
            serde_json::json!({}),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let agents = payload.as_array().cloned().unwrap_or_default();

    if agents.is_empty() {
        printer.line("No agents running.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = agents
        .iter()
        .map(|a| {
            let id = a.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?");
            let short_id = &id[..id.len().min(8)];
            vec![
                short_id.to_string(),
                a.get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                a.get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("—")
                    .to_string(),
                a.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                a.get("cli_provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                task_label(a),
            ]
        })
        .collect();

    printer.table(
        &["ID (short)", "TYPE", "ROLE", "STATUS", "PROVIDER", "TASK"],
        rows,
    );
    Ok(())
}

fn task_label(a: &serde_json::Value) -> String {
    a.get("task_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|id| id[..id.len().min(8)].to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub async fn spawn(
    task: &str,
    role: Option<&str>,
    parent_id: Option<Uuid>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let mut params = serde_json::json!({ "task": task });
    if let Some(r) = role {
        params["role"] = serde_json::Value::String(r.to_string());
    }
    if let Some(pid) = parent_id {
        params["parent_id"] = serde_json::Value::String(pid.to_string());
    }

    let payload = client
        .request(Some(&anchor.project_root), "agents.spawn", params)
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let id = payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    printer.line(&format!("Splinter spawned: {id}"));
    Ok(())
}

pub async fn pause(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "agents.pause",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;
    printer.line(&format!("Agent {agent_id} paused."));
    Ok(())
}

pub async fn resume(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "agents.resume",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;
    printer.line(&format!("Agent {agent_id} resumed."));
    Ok(())
}

pub async fn kill(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "agents.kill",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;
    printer.line(&format!("Agent {agent_id} killed."));
    Ok(())
}

pub async fn terminate(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "agents.terminate",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;
    printer.line(&format!("Agent {agent_id} terminated."));
    Ok(())
}

pub async fn failover(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "agents.failover",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let provider = payload
        .get("new_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("next in cascade");
    printer.line(&format!("Agent {agent_id} failing over to {provider}."));
    Ok(())
}
