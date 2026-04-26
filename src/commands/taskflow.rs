use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};

pub async fn status(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.status",
            serde_json::json!({}),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let project = payload.get("project");
    let name = project
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("AegisCore");

    println!("{} Pipeline Status", name);
    printer.separator();

    let milestones = payload.get("milestones").and_then(|v| v.as_object());
    if let Some(m_map) = milestones {
        let mut keys: Vec<_> = m_map.keys().collect();
        keys.sort();

        for key in keys {
            let m = m_map.get(key).unwrap();
            let status = m
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            let ok = status == "done";
            printer.status_line(ok, key, status);
        }
    } else {
        printer.line("No milestones found.");
    }

    Ok(())
}

pub async fn list(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    // Re-use status for list for now, or refine if needed
    status(printer, client, anchor).await
}

pub async fn show(
    milestone_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.show",
            serde_json::json!(milestone_id),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(milestone_id);
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");

    println!("Milestone: {} [{}]", name, status);
    printer.separator();

    let tasks = payload.get("tasks").and_then(|v| v.as_array());
    if let Some(task_list) = tasks {
        for t in task_list {
            let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let task = t.get("task").and_then(|v| v.as_str()).unwrap_or("?");
            let st = t
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            let reg_id = t
                .get("registry_task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mark = match st {
                "done" => "[✓]",
                "in-progress" => "[>]",
                "blocked" => "[X]",
                _ => "[ ]",
            };

            println!(
                "  {} {} - {} {}",
                mark,
                id,
                task,
                if reg_id.is_empty() {
                    "".to_string()
                } else {
                    format!("({})", &reg_id[..8])
                }
            );
        }
    } else {
        printer.line("No tasks found for this milestone.");
    }

    Ok(())
}

pub async fn sync(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.sync",
            serde_json::json!({}),
        )
        .await?;

    let updated = payload
        .get("updated_tasks")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    printer.line(&format!("Taskflow synced. {} tasks updated.", updated));
    Ok(())
}

pub async fn assign(
    roadmap_id: &str,
    task_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "taskflow.assign",
            serde_json::json!({ "roadmap_id": roadmap_id, "task_id": task_id }),
        )
        .await?;

    printer.line(&format!(
        "Roadmap task {roadmap_id} linked to registry task {task_id}."
    ));
    Ok(())
}

pub async fn create_milestone(
    id: &str,
    name: &str,
    lld: Option<&str>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "taskflow.create_milestone",
            serde_json::json!({
                "id": id,
                "name": name,
                "lld": lld,
            }),
        )
        .await?;

    printer.line(&format!("Milestone {id} ({name}) created."));
    Ok(())
}

pub async fn add_task(
    milestone_id: &str,
    id: &str,
    task: &str,
    task_type: aegis_taskflow::model::TaskType,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "taskflow.add_task",
            serde_json::json!({
                "milestone_id": milestone_id,
                "id": id,
                "task": task,
                "task_type": task_type,
            }),
        )
        .await?;

    printer.line(&format!(
        "Task {id} added to {} [{:?}].",
        milestone_id, task_type
    ));
    Ok(())
}

pub async fn set_task_status(
    milestone_id: &str,
    task_id: &str,
    status: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    client
        .request(
            Some(&anchor.project_root),
            "taskflow.set_task_status",
            serde_json::json!({
                "milestone_id": milestone_id,
                "task_id": task_id,
                "status": status,
            }),
        )
        .await?;

    printer.line(&format!(
        "Task {task_id} in milestone {milestone_id} marked {status}."
    ));
    Ok(())
}

pub async fn next(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.next",
            serde_json::json!({}),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let outcome = payload.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
    match outcome {
        "ready" => {
            let id = payload.get("milestone_id").and_then(|v| v.as_str()).unwrap_or("?");
            let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let count = payload.get("task_count").and_then(|v| v.as_u64()).unwrap_or(0);
            printer.line(&format!("Next milestone: {id} — {name} ({count} tasks pending)"));
        }
        "exhausted" => printer.line("No pending milestones — backlog is exhausted."),
        "blocked" => {
            let deps: Vec<&str> = payload
                .get("waiting_on")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            printer.line(&format!("Blocked — waiting on: {}", deps.join(", ")));
        }
        _ => printer.line(&format!("Unexpected response: {payload}")),
    }
    Ok(())
}
