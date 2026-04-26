use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};
use clap::ValueEnum;
use uuid::Uuid;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum MessageKindArg {
    Task,
    Handoff,
    Notification,
    Command,
}

impl From<MessageKindArg> for &'static str {
    fn from(value: MessageKindArg) -> Self {
        match value {
            MessageKindArg::Task => "task",
            MessageKindArg::Handoff => "handoff",
            MessageKindArg::Notification => "notification",
            MessageKindArg::Command => "command",
        }
    }
}

pub async fn send(
    to_agent_id: &str,
    message: &str,
    from_agent_id: Option<Uuid>,
    kind: MessageKindArg,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let mut params = serde_json::json!({
        "to_agent_id": to_agent_id,
        "message": message,
        "kind": <MessageKindArg as Into<&'static str>>::into(kind),
    });

    if let Some(from) = from_agent_id {
        params["from_agent_id"] = serde_json::Value::String(from.to_string());
    }

    let payload = client
        .request(Some(&anchor.project_root), "message.send", params)
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let message_id = payload
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let nudged = payload
        .get("nudged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let warning = payload.get("warning").and_then(|v| v.as_str());

    printer.line(&format!("Queued message {message_id} for {to_agent_id}."));
    if nudged {
        printer.line("Recipient was nudged to inspect its inbox.");
    }
    if let Some(warning) = warning {
        printer.line(&format!("Warning: {warning}"));
    }
    Ok(())
}

pub async fn inbox(
    agent_id: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "message.inbox",
            serde_json::json!({ "agent_id": agent_id }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let agent_name = payload
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or(agent_id);
    let agent_status = payload
        .get("agent_status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let messages = payload
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    printer.kv(&[
        ("Agent:", agent_name),
        ("ID:", agent_id),
        ("Status:", agent_status),
        ("Messages:", &messages.len().to_string()),
    ]);

    if messages.is_empty() {
        printer.line("Inbox is empty.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = messages.iter().map(message_row).collect();
    printer.table(&["TIME", "FROM", "KIND", "PAYLOAD"], rows);
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
                "message.list",
                serde_json::json!({ "agent_id": agent_id }),
            )
            .await?
    } else {
        client
            .request(
                Some(&anchor.project_root),
                "message.list",
                serde_json::Value::Null,
            )
            .await?
    };

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    if payload.is_object() {
        let agent_name = payload
            .get("agent_name")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let agent_status = payload
            .get("agent_status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let messages = payload
            .get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        printer.kv(&[
            ("Agent:", agent_name),
            ("Status:", agent_status),
            ("Messages:", &messages.len().to_string()),
        ]);

        if messages.is_empty() {
            printer.line("Inbox is empty.");
            return Ok(());
        }

        let rows: Vec<Vec<String>> = messages.iter().map(message_row).collect();
        printer.table(&["TIME", "FROM", "KIND", "PAYLOAD"], rows);
        return Ok(());
    }

    let inboxes: Vec<_> = payload
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| {
            entry
                .get("queued_messages")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                > 0
        })
        .collect();
    if inboxes.is_empty() {
        printer.line("No inboxes with queued messages.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = inboxes
        .iter()
        .map(|entry| {
            vec![
                short_id(
                    entry
                        .get("agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?"),
                )
                .to_string(),
                entry
                    .get("agent_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                entry
                    .get("agent_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                entry
                    .get("queued_messages")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    .to_string(),
                entry
                    .get("newest_message_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("—")
                    .to_string(),
            ]
        })
        .collect();

    printer.table(&["ID", "AGENT", "STATUS", "QUEUED", "LATEST"], rows);
    Ok(())
}

fn message_row(message: &serde_json::Value) -> Vec<String> {
    vec![
        message
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        render_from(message.get("from")),
        message
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        render_payload(message.get("payload")),
    ]
}

fn render_from(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::Object(map)) => map
            .get("Agent")
            .and_then(|v| v.as_str())
            .map(|agent| format!("agent:{agent}"))
            .unwrap_or_else(|| "agent".to_string()),
        Some(serde_json::Value::String(text)) => text.to_string(),
        Some(serde_json::Value::Null) => "system".to_string(),
        Some(other) => other.to_string(),
        None => "?".to_string(),
    }
}

fn render_payload(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => {
            let text = other.to_string();
            if text.chars().count() > 72 {
                text.chars().take(72).collect::<String>() + "…"
            } else {
                text
            }
        }
        None => "?".to_string(),
    }
}

fn short_id(raw: &str) -> &str {
    &raw[..raw.len().min(8)]
}
