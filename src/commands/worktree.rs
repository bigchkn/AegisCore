use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};

pub async fn create(
    milestone_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "worktree.create",
            serde_json::json!({ "milestone_id": milestone_id }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or("?");
    let branch = payload
        .get("branch")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    printer.line(&format!("Worktree created: {path}  branch={branch}"));
    Ok(())
}

pub async fn merge(
    milestone_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "worktree.merge",
            serde_json::json!({ "milestone_id": milestone_id }),
        )
        .await?;

    if printer.format != crate::output::OutputFormat::Json {
        printer.line(&format!("Milestone {milestone_id} merged into main."));
    }
    Ok(())
}

pub async fn list(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "worktree.list",
            serde_json::json!({}),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let arr = payload.as_array().cloned().unwrap_or_default();
    if arr.is_empty() {
        printer.line("No active milestone worktrees.");
        return Ok(());
    }

    println!("{:<12}  {}", "MILESTONE", "PATH");
    printer.separator();
    for entry in &arr {
        let id = entry
            .get("milestone_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("?");
        println!("{:<12}  {}", id, path);
    }
    Ok(())
}
