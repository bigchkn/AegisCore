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
            "project.status",
            serde_json::json!({}),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let root = payload
        .get("project_root")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let session = payload
        .get("session_name")
        .and_then(|v| v.as_str())
        .unwrap_or("aegis");

    let agents = payload.get("agents");
    let active = agents
        .and_then(|a| a.get("active"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let queued = agents
        .and_then(|a| a.get("queued"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = agents
        .and_then(|a| a.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let tasks = payload.get("tasks");
    let t_active = tasks
        .and_then(|a| a.get("active"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let t_complete = tasks
        .and_then(|a| a.get("complete"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let t_failed = tasks
        .and_then(|a| a.get("failed"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let interval = payload
        .get("watchdog")
        .and_then(|w| w.get("interval_ms"))
        .and_then(|v| v.as_u64())
        .unwrap_or(2000);

    let providers: Vec<String> = payload
        .get("providers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    printer.kv(&[
        ("Project:", root),
        ("Session:", session),
        (
            "Agents:",
            &format!("{active} active · {queued} queued · {total} total"),
        ),
        (
            "Tasks:",
            &format!("{t_active} active · {t_complete} complete · {t_failed} failed"),
        ),
        ("Watchdog:", &format!("polling every {interval}ms")),
        ("Providers:", &providers.join(" → ")),
    ]);
    Ok(())
}

pub async fn logs(
    agent_id: &str,
    lines: Option<usize>,
    follow: bool,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let params = serde_json::json!({
        "agent_id": agent_id,
        "lines": lines.unwrap_or(50),
        "follow": follow,
    });

    if !follow {
        let payload = client
            .request(Some(&anchor.project_root), "logs.tail", params)
            .await?;

        if printer.format == crate::output::OutputFormat::Json {
            printer.json(&payload);
            return Ok(());
        }

        if let Some(lines_arr) = payload.as_array() {
            for line in lines_arr {
                if let Some(s) = line.as_str() {
                    println!("{s}");
                }
            }
        } else if let Some(s) = payload.as_str() {
            print!("{s}");
        }
    } else {
        // Subscribe mode: send follow request then stream lines until Ctrl+C
        client
            .subscribe_lines(|line| {
                // Each line is either a log line string or a JSON event
                // Just print raw for now — daemon shapes this
                println!("{line}");
                true
            })
            .await?;
    }

    Ok(())
}
