use crate::{
    anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer,
};
use aegis_core::SandboxNetworkPolicy;
use aegis_design::{BootstrapContext, DesignEngine, TemplateRegistry};
use std::collections::HashMap;

fn parse_vars(raw: &[String]) -> Result<HashMap<String, String>, AegisCliError> {
    let mut map = HashMap::new();
    for item in raw {
        let (k, v) = item.split_once('=').ok_or_else(|| {
            AegisCliError::Unexpected(format!("--var must be KEY=VALUE, got: {item}"))
        })?;
        map.insert(k.to_owned(), v.to_owned());
    }
    Ok(map)
}

pub fn list(printer: &Printer, anchor: &ProjectAnchor) -> Result<(), AegisCliError> {
    let reg = TemplateRegistry::load(&anchor.project_root);
    let items = reg.list();

    if printer.format == crate::output::OutputFormat::Json {
        let arr: Vec<_> = items
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "kind": format!("{:?}", t.template.metadata.kind).to_lowercase(),
                    "layer": t.layer.to_string(),
                    "description": t.template.metadata.description,
                })
            })
            .collect();
        printer.json(&serde_json::Value::Array(arr));
        return Ok(());
    }

    println!("{:<30} {:<10} {:<10}  DESCRIPTION", "NAME", "KIND", "LAYER");
    printer.separator();
    for t in &items {
        println!(
            "{:<30} {:<10} {:<10}  {}",
            t.name,
            format!("{:?}", t.template.metadata.kind).to_lowercase(),
            t.layer,
            t.template.metadata.description,
        );
    }
    Ok(())
}

pub fn show(name: &str, printer: &Printer, anchor: &ProjectAnchor) -> Result<(), AegisCliError> {
    let reg = TemplateRegistry::load(&anchor.project_root);
    let resolved = reg
        .get(name)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;
    let t = &resolved.template;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&serde_json::json!({
            "name": t.metadata.name,
            "kind": format!("{:?}", t.metadata.kind).to_lowercase(),
            "version": t.metadata.version,
            "layer": resolved.layer.to_string(),
            "description": t.metadata.description,
            "tags": t.metadata.tags,
            "agent": {
                "role": t.agent.role,
                "cli_provider": t.agent.cli_provider,
                "model": t.agent.model,
                "auto_cleanup": t.agent.auto_cleanup,
                "fallback_cascade": t.agent.fallback_cascade,
            },
            "variables": {
                "required": t.variables.required,
                "optional": t.variables.optional,
            },
            "system_prompt_preview": t.system_prompt.lines().take(10).collect::<Vec<_>>().join("\n"),
        }));
        return Ok(());
    }

    println!("Template: {} ({})", t.metadata.name, resolved.layer);
    println!("Kind:     {:?}", t.metadata.kind);
    println!("Version:  {}", t.metadata.version);
    println!("Tags:     {}", t.metadata.tags.join(", "));
    printer.separator();
    println!("Agent:");
    println!("  provider:  {}", t.agent.cli_provider);
    if let Some(m) = &t.agent.model {
        println!("  model:     {m}");
    }
    println!("  role:      {}", t.agent.role);
    println!("  cleanup:   {}", t.agent.auto_cleanup);
    printer.separator();
    println!("Variables:");
    println!("  required: {}", t.variables.required.join(", "));
    println!("  optional: {}", t.variables.optional.join(", "));
    printer.separator();
    println!("System Prompt (first 10 lines):");
    for line in t.system_prompt.lines().take(10) {
        println!("  {line}");
    }
    Ok(())
}

pub async fn spawn(
    name: &str,
    model_override: Option<&str>,
    extra_vars: &[String],
    printer: &Printer,
    client: &DaemonClient,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let cli_vars = parse_vars(extra_vars)?;
    let reg = TemplateRegistry::load(&anchor.project_root);
    let resolved = reg
        .get(name)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    let vars = BootstrapContext::build(&resolved.template, &anchor.project_root, &cli_vars, None)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    let mut rendered = DesignEngine::render(&resolved.template, &vars)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    if let Some(m) = model_override {
        rendered.model = Some(m.to_owned());
    }

    let payload = client
        .request(
            Some(&anchor.project_root),
            "design.spawn",
            serde_json::to_value(&rendered).expect("RenderedTemplate serializes"),
        )
        .await?;

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&payload);
        return Ok(());
    }

    let agent_id = payload
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let role = payload
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or(&rendered.role);

    printer.line(&format!("Agent spawned: {agent_id}  role={role}"));
    Ok(())
}

pub fn apply(
    name: &str,
    role_override: Option<&str>,
    extra_vars: &[String],
    printer: &Printer,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let cli_vars = parse_vars(extra_vars)?;
    let reg = TemplateRegistry::load(&anchor.project_root);
    let resolved = reg
        .get(name)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    let vars = BootstrapContext::build(&resolved.template, &anchor.project_root, &cli_vars, None)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    let rendered = DesignEngine::render(&resolved.template, &vars)
        .map_err(|e| AegisCliError::Unexpected(e.to_string()))?;

    let role = role_override.unwrap_or(&rendered.role);

    let network_str = match rendered.sandbox_network {
        SandboxNetworkPolicy::None => "none",
        SandboxNetworkPolicy::OutboundOnly => "outbound_only",
        SandboxNetworkPolicy::Any => "any",
    };

    let kind_str = format!("{:?}", rendered.kind).to_lowercase();
    let model_line = rendered
        .model
        .as_deref()
        .map(|m| format!("model = \"{m}\"\n"))
        .unwrap_or_default();

    let block = format!(
        "# Generated by: aegis design apply {name}\n# Template: {layer}/{name} v{ver}\n[agent.{role}]\ntype = \"{kind_str}\"\nrole = \"{role}\"\ncli_provider = \"{provider}\"\nauto_cleanup = {cleanup}\n{model_line}[agent.{role}.sandbox]\nnetwork = \"{network_str}\"\n",
        layer = resolved.layer,
        ver = resolved.template.metadata.version,
        provider = rendered.cli_provider,
        cleanup = rendered.auto_cleanup,
    );

    if printer.format == crate::output::OutputFormat::Json {
        printer.json(&serde_json::json!({ "toml": block }));
        return Ok(());
    }

    println!("{block}");
    printer.line(&format!(
        "Paste the above into aegis.toml, then run `aegis start --bastion {role}`"
    ));
    Ok(())
}

pub fn new(
    name: &str,
    kind: &str,
    printer: &Printer,
    anchor: &ProjectAnchor,
) -> Result<(), AegisCliError> {
    let template_dir = anchor.aegis_dir.join("templates").join(name);

    if template_dir.exists() {
        return Err(AegisCliError::Unexpected(format!(
            "template already exists at {}",
            template_dir.display()
        )));
    }
    std::fs::create_dir_all(&template_dir).map_err(AegisCliError::Io)?;

    let toml_content = format!(
        r#"[template]
name = "{name}"
description = "TODO: describe what this template does"
kind = "{kind}"
version = "1"
tags = []

[agent]
role = "{name}"
cli_provider = "claude-code"
# model = "sonnet"
auto_cleanup = false
fallback_cascade = []

[agent.sandbox]
network = "outbound_only"

[variables]
required = ["project_root"]
optional = []
"#
    );

    let prompt_content = format!(
        "# {name}\n\nYou are a {{{{role}}}} agent operating in `{{{{project_root}}}}`.\n\nTODO: describe the agent's role and capabilities.\n"
    );

    std::fs::write(template_dir.join("template.toml"), toml_content).map_err(AegisCliError::Io)?;
    std::fs::write(template_dir.join("system_prompt.md"), prompt_content)
        .map_err(AegisCliError::Io)?;
    std::fs::write(
        template_dir.join("startup.md"),
        "TODO: first instruction sent to the agent after it starts.\n",
    )
    .map_err(AegisCliError::Io)?;

    if printer.format != crate::output::OutputFormat::Json {
        printer.line(&format!(
            "Template scaffolded at {}",
            template_dir.display()
        ));
        printer
            .line("Edit template.toml, system_prompt.md, and startup.md to define your template.");
    }
    Ok(())
}
