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
    let status_payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.status",
            serde_json::json!({}),
        )
        .await?;
    let backlog_payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.show",
            serde_json::json!("backlog"),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&serde_json::json!({
            "project": status_payload.get("project").cloned().unwrap_or(serde_json::Value::Null),
            "backlog": backlog_payload,
            "milestones": status_payload.get("milestones").cloned().unwrap_or(serde_json::Value::Null),
        }));
        return Ok(());
    }

    let project = status_payload.get("project");
    let name = project
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("AegisCore");

    println!("{} Taskflow List", name);
    printer.separator();
    printer.table(
        &["ID", "Name", "Status", "Tasks"],
        taskflow_list_rows(&status_payload, &backlog_payload),
    );
    Ok(())
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

fn taskflow_list_rows(
    status_payload: &serde_json::Value,
    backlog_payload: &serde_json::Value,
) -> Vec<Vec<String>> {
    let mut rows = Vec::new();

    rows.push(vec![
        "backlog".to_string(),
        backlog_payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Global Backlog")
            .to_string(),
        "n/a".to_string(),
        task_summary(backlog_payload),
    ]);

    if let Some(milestones) = status_payload.get("milestones").and_then(|v| v.as_object()) {
        let mut keys: Vec<_> = milestones.keys().collect();
        keys.sort();
        for key in keys {
            let milestone = milestones.get(key).unwrap();
            rows.push(vec![
                key.to_string(),
                milestone
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                milestone
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pending")
                    .to_string(),
                "-".to_string(),
            ]);
        }
    }

    rows
}

fn task_summary(payload: &serde_json::Value) -> String {
    let Some(tasks) = payload.get("tasks").and_then(|v| v.as_array()) else {
        return "no tasks".to_string();
    };
    let total = tasks.len();
    if total == 0 {
        return "no tasks".to_string();
    }

    let pending = tasks
        .iter()
        .filter(|task| {
            task.get("status")
                .and_then(|v| v.as_str())
                .map(|status| status != "done")
                .unwrap_or(true)
        })
        .count();

    let total_label = if total == 1 { "task" } else { "tasks" };
    match pending {
        0 => format!("{total} {total_label}, none pending"),
        1 => format!("{total} {total_label}, 1 pending"),
        _ => format!("{total} {total_label}, {pending} pending"),
    }
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

    let outcome = payload
        .get("outcome")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match outcome {
        "ready" => {
            let id = payload
                .get("milestone_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let count = payload
                .get("task_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if id == "backlog" {
                printer.line(&format!("Next backlog: {name} ({count} tasks pending)"));
            } else {
                printer.line(&format!(
                    "Next milestone: {id} — {name} ({count} tasks pending)"
                ));
            }
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

pub async fn notify(
    event: &str,
    message: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "taskflow.notify",
            serde_json::json!({ "event": event, "message": message }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let count = payload
        .get("notified")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if count == 0 {
        printer.line("No active bastion agents found — notification not sent.");
    } else {
        printer.line(&format!("Notified {count} bastion agent(s): event={event}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn task_summary_reports_empty_backlog() {
        assert_eq!(task_summary(&json!({ "tasks": [] })), "no tasks");
    }

    #[test]
    fn task_summary_counts_pending_backlog_tasks() {
        let payload = json!({
            "tasks": [
                { "status": "pending" },
                { "status": "done" },
                { "status": "blocked" }
            ]
        });

        assert_eq!(task_summary(&payload), "3 tasks, 2 pending");
    }

    #[test]
    fn taskflow_list_rows_include_backlog_before_milestones() {
        let status = json!({
            "milestones": {
                "M2": { "name": "Second", "status": "pending" },
                "M1": { "name": "First", "status": "done" }
            }
        });
        let backlog = json!({
            "name": "Global Backlog",
            "tasks": [{ "status": "pending" }]
        });

        let rows = taskflow_list_rows(&status, &backlog);

        assert_eq!(
            rows[0],
            vec!["backlog", "Global Backlog", "n/a", "1 task, 1 pending"]
        );
        assert_eq!(rows[1], vec!["M1", "First", "done", "-"]);
        assert_eq!(rows[2], vec!["M2", "Second", "pending", "-"]);
    }
}
