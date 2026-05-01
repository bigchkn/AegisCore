use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};

#[allow(clippy::too_many_arguments)]
pub async fn request(
    agent_id: &str,
    task_id: Option<&str>,
    question: &str,
    context: Option<&str>,
    priority: i32,
    wait_for_answer: bool,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let mut params = serde_json::json!({
        "agent_id": agent_id,
        "question": question,
        "context": context
            .map(|value| serde_json::Value::String(value.to_string()))
            .unwrap_or_else(|| serde_json::json!({})),
        "priority": priority,
    });

    if let Some(task_id) = task_id {
        params["task_id"] = serde_json::Value::String(task_id.to_string());
    }

    let payload = client
        .request(Some(&anchor.project_root), "clarify.request", params)
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
    } else {
        let request_id = payload
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        printer.line(&format!("Clarification request queued: {request_id}"));
    }

    if wait_for_answer {
        let request_id = payload
            .get("request_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AegisCliError::DaemonError("clarification request missing request_id".into())
            })?;
        wait_for_response(request_id, None, printer, client, anchor).await?;
    }

    Ok(())
}

pub async fn list(
    agent_id: Option<&str>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = if let Some(agent_id) = agent_id {
        client
            .request(
                Some(&anchor.project_root),
                "clarify.list",
                serde_json::json!({ "agent_id": agent_id }),
            )
            .await?
    } else {
        client
            .request(
                Some(&anchor.project_root),
                "clarify.list",
                serde_json::Value::Null,
            )
            .await?
    };

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let requests = payload.as_array().cloned().unwrap_or_default();
    if requests.is_empty() {
        printer.line("No clarification requests.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = requests
        .iter()
        .map(|request| {
            vec![
                short_id(
                    request
                        .get("request_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?"),
                )
                .to_string(),
                short_id(
                    request
                        .get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?"),
                )
                .to_string(),
                request
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                request
                    .get("priority")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .to_string(),
                request
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
            ]
        })
        .collect();

    printer.table(&["REQUEST", "AGENT", "STATUS", "PRI", "QUESTION"], rows);
    Ok(())
}

pub async fn show(
    request_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "clarify.show",
            serde_json::json!({ "request_id": request_id }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let priority = payload
        .get("priority")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .to_string();
    let question = payload
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();

    printer.kv(&[
        (
            "Request:",
            payload
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
        ),
        (
            "Agent:",
            payload
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
        ),
        (
            "Status:",
            payload
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
        ),
        ("Priority:", &priority),
        ("Question:", &question),
    ]);

    if let Some(response) = payload.get("response") {
        printer.line(&format!(
            "Answer: {}",
            response
                .get("answer")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ));
    }

    Ok(())
}

pub async fn answer(
    request_id: &str,
    answer: &str,
    payload: Option<&str>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload_value = match payload {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str::<serde_json::Value>(raw)
            .map_err(|e| {
                AegisCliError::InvalidArg(format!("invalid clarification payload JSON: {e}"))
            })?,
        _ => serde_json::json!({}),
    };

    let response = client
        .request(
            Some(&anchor.project_root),
            "clarify.answer",
            serde_json::json!({
                "request_id": request_id,
                "answer": answer,
                "payload": payload_value,
                "answered_by": "human_cli",
            }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&response);
        return Ok(());
    }

    printer.line(&format!("Clarification {request_id} answered."));
    if let Some(error) = response.get("delivery_error").and_then(|v| v.as_str()) {
        printer.line(&format!("Warning: {error}"));
    }
    Ok(())
}

pub async fn wait_for_response(
    request_or_agent_id: &str,
    timeout_secs: Option<u64>,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let mut params = serde_json::json!({
        "request_id": request_or_agent_id,
    });
    if let Some(timeout_secs) = timeout_secs {
        params["timeout_secs"] = serde_json::json!(timeout_secs);
    }

    let payload = client
        .request(Some(&anchor.project_root), "clarify.wait", params)
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    printer.line(&format!(
        "Clarification {} is now {}.",
        payload
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or(request_or_agent_id),
        payload
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    ));
    Ok(())
}

fn short_id(id: &str) -> &str {
    let end = id.len().min(8);
    &id[..end]
}
