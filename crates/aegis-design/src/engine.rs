use crate::error::{DesignError, Result};
use crate::template::{Template, TemplateKind};
use aegis_core::SandboxNetworkPolicy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedTemplate {
    pub name: String,
    pub kind: TemplateKind,
    pub role: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub task_description: Option<String>,
    pub cli_provider: String,
    pub model: Option<String>,
    pub auto_cleanup: bool,
    pub fallback_cascade: Vec<String>,
    pub sandbox_network: SandboxNetworkPolicy,
    pub system_prompt: String,
    pub startup: Option<String>,
}

pub struct DesignEngine;

impl DesignEngine {
    /// Render a template against the provided variable map.
    ///
    /// Required variables not present in `vars` produce an error.
    /// Optional variables not present are substituted with an empty string.
    /// After substitution, any remaining `{{...}}` placeholders are an error.
    pub fn render(template: &Template, vars: &HashMap<String, String>) -> Result<RenderedTemplate> {
        // Build full var map: start with empty strings for all optionals, then overlay provided vars.
        let mut resolved: HashMap<String, String> = HashMap::new();
        for opt in &template.variables.optional {
            resolved.insert(opt.clone(), String::new());
        }
        resolved.extend(vars.iter().map(|(k, v)| (k.clone(), v.clone())));

        // Validate required vars are present.
        for req in &template.variables.required {
            if !resolved.contains_key(req.as_str()) {
                return Err(DesignError::UnresolvedRequired { name: req.clone() });
            }
        }

        let system_prompt = render_string(&template.system_prompt, &resolved)?;
        let startup = template
            .startup
            .as_deref()
            .map(|s| render_string(s, &resolved))
            .transpose()?;

        let sandbox_network = match template
            .agent
            .sandbox
            .network
            .as_deref()
            .unwrap_or("outbound_only")
        {
            "none" => SandboxNetworkPolicy::None,
            "any" => SandboxNetworkPolicy::Any,
            _ => SandboxNetworkPolicy::OutboundOnly,
        };

        Ok(RenderedTemplate {
            name: template.metadata.name.clone(),
            kind: template.metadata.kind.clone(),
            role: template.agent.role.clone(),
            task_id: resolved.get("task_id").filter(|v| !v.is_empty()).cloned(),
            task_description: resolved
                .get("task_description")
                .or_else(|| resolved.get("doc_description"))
                .filter(|v| !v.is_empty())
                .cloned(),
            cli_provider: template.agent.cli_provider.clone(),
            model: template.agent.model.clone(),
            auto_cleanup: template.agent.auto_cleanup,
            fallback_cascade: template.agent.fallback_cascade.clone(),
            sandbox_network,
            system_prompt,
            startup,
        })
    }
}

fn render_string(input: &str, vars: &HashMap<String, String>) -> Result<String> {
    let mut output = input.to_owned();
    for (key, value) in vars {
        output = output.replace(&format!("{{{{{key}}}}}"), value);
    }

    // Check for any remaining {{...}} placeholders.
    let re = Regex::new(r"\{\{[^}]+\}\}").expect("static regex");
    let remaining: Vec<String> = re
        .find_iter(&output)
        .map(|m| m.as_str().to_owned())
        .collect();
    if !remaining.is_empty() {
        return Err(DesignError::UnresolvedPlaceholders {
            placeholders: remaining,
        });
    }

    Ok(output)
}
