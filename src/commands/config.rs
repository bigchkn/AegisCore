use std::path::Path;
use crate::{anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer};

pub fn validate(anchor: &ProjectAnchor, printer: &Printer) -> Result<(), AegisCliError> {
    use aegis_core::config::EffectiveConfig;

    let global = EffectiveConfig::load_global().unwrap_or_default();
    let project = EffectiveConfig::load_project(&anchor.project_root)
        .map_err(|e| AegisCliError::Config(e.to_string()))?;
    let effective = EffectiveConfig::resolve(&global, &project)
        .map_err(|e| AegisCliError::Config(e.to_string()))?;

    let errors = effective.validate();
    if errors.is_empty() {
        printer.line("aegis.toml is valid.");
        Ok(())
    } else {
        for e in &errors {
            eprintln!("error: field `{}`: {}", e.field, e.reason);
        }
        Err(AegisCliError::Config(format!("{} validation error(s)", errors.len())))
    }
}

pub async fn show(
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    // Prefer daemon's merged view; fall back to local resolution if daemon is down.
    match client
        .request(Some(&anchor.project_root), "config.show", serde_json::json!({}))
        .await
    {
        Ok(payload) => {
            printer.json(&payload);
        }
        Err(AegisCliError::DaemonNotRunning) => {
            show_local(&anchor.project_root, printer)?;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

fn show_local(project_root: &Path, printer: &Printer) -> Result<(), AegisCliError> {
    use aegis_core::config::EffectiveConfig;
    let global = EffectiveConfig::load_global().unwrap_or_default();
    let project = EffectiveConfig::load_project(project_root).unwrap_or_default();
    let effective = EffectiveConfig::resolve(&global, &project)
        .map_err(|e| AegisCliError::Config(e.to_string()))?;
    let value = serde_json::to_value(&effective)
        .map_err(|e| AegisCliError::Config(e.to_string()))?;
    printer.json(&value);
    Ok(())
}
