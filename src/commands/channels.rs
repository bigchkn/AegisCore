use crate::{anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer};

pub async fn add_telegram(
    token: Option<&str>,
    chat_ids: &[i64],
    yes: bool,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let resolved_token = match token {
        Some(t) => t.to_string(),
        None => {
            if !yes {
                prompt_input("Telegram bot token: ")?
            } else {
                return Err(AegisCliError::InvalidArg(
                    "--token required when --yes is set".into(),
                ));
            }
        }
    };

    let resolved_chat_ids: Vec<i64> = if !chat_ids.is_empty() {
        chat_ids.to_vec()
    } else if !yes {
        let raw = prompt_input("Allowed chat IDs (comma-separated): ")?;
        raw.split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect()
    } else {
        return Err(AegisCliError::InvalidArg(
            "--chat-id required when --yes is set".into(),
        ));
    };

    let payload = client
        .request(
            Some(&anchor.project_root),
            "channel.add",
            serde_json::json!({
                "kind": "telegram",
                "config": {
                    "token": resolved_token,
                    "chat_ids": resolved_chat_ids
                }
            }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
    } else {
        printer.line("Telegram channel added and activated.");
    }
    Ok(())
}

pub async fn add_mailbox(
    name: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "channel.add",
            serde_json::json!({ "kind": "mailbox", "name": name }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
    } else {
        printer.line(&format!("Mailbox channel '{name}' added."));
    }
    Ok(())
}

pub async fn list(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(Some(&anchor.project_root), "channel.list", serde_json::json!({}))
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let channels = payload.as_array().cloned().unwrap_or_default();
    if channels.is_empty() {
        printer.line("No channels configured.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = channels
        .iter()
        .map(|c| {
            vec![
                c.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                c.get("kind").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                c.get("status").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            ]
        })
        .collect();

    printer.table(&["NAME", "TYPE", "STATUS"], rows);
    Ok(())
}

pub async fn channel_status(
    name: &str,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let payload = client
        .request(
            Some(&anchor.project_root),
            "channel.status",
            serde_json::json!({ "name": name }),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    let messages = payload.get("messages_queued").and_then(|v| v.as_u64()).unwrap_or(0);
    printer.kv(&[
        ("Channel:", name),
        ("Status:", status),
        ("Messages queued:", &messages.to_string()),
    ]);
    Ok(())
}

pub async fn remove(
    name: &str,
    yes: bool,
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    if !yes {
        let confirm = prompt_input(&format!(
            "Remove channel '{name}'? This will deactivate it. [y/N] "
        ))?;
        if confirm.trim().to_lowercase() != "y" {
            printer.line("Cancelled.");
            return Ok(());
        }
    }

    client
        .request(
            Some(&anchor.project_root),
            "channel.remove",
            serde_json::json!({ "name": name }),
        )
        .await?;

    printer.line(&format!("Channel '{name}' removed."));
    Ok(())
}

fn prompt_input(prompt: &str) -> Result<String, AegisCliError> {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}
