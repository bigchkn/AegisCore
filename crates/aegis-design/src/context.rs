use crate::error::{DesignError, Result};
use crate::template::Template;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Resolves template variables from project state and caller-supplied overrides.
pub struct BootstrapContext;

impl BootstrapContext {
    /// Build a variable map for rendering a template.
    ///
    /// Resolution order (later wins):
    /// 1. Standard variables (`project_root`)
    /// 2. Taskflow variables read from roadmap `index.toml` (when present)
    /// 3. `bastion_agent_id` (supplied by dispatcher at splinter spawn time)
    /// 4. Caller-supplied CLI overrides (`--var KEY=VALUE`)
    pub fn build(
        _template: &Template,
        project_root: &Path,
        cli_vars: &HashMap<String, String>,
        bastion_agent_id: Option<&str>,
    ) -> Result<HashMap<String, String>> {
        let mut vars: HashMap<String, String> = HashMap::new();

        vars.insert(
            "project_root".into(),
            project_root.to_string_lossy().into_owned(),
        );

        // Taskflow vars — best effort; missing index is not fatal for non-taskflow templates.
        if let Ok(tf_vars) = Self::load_taskflow_vars(project_root) {
            vars.extend(tf_vars);
        }

        if let Some(bid) = bastion_agent_id {
            vars.insert("bastion_agent_id".into(), bid.to_owned());
        }

        // CLI overrides take highest priority.
        vars.extend(cli_vars.iter().map(|(k, v)| (k.clone(), v.clone())));

        Ok(vars)
    }

    fn load_taskflow_vars(project_root: &Path) -> Result<HashMap<String, String>> {
        let index_path = project_root
            .join(".aegis")
            .join("designs")
            .join("roadmap")
            .join("index.toml");

        if !index_path.exists() {
            return Err(DesignError::IndexNotFound {
                path: index_path.to_string_lossy().into_owned(),
            });
        }

        let index_str = std::fs::read_to_string(&index_path).map_err(|e| DesignError::Io {
            path: index_path.to_string_lossy().into_owned(),
            reason: e.to_string(),
        })?;

        let index: RawIndex =
            toml::from_str(&index_str).map_err(|e| DesignError::Io {
                path: index_path.to_string_lossy().into_owned(),
                reason: e.to_string(),
            })?;

        let milestone_id = index.project.current_milestone;
        let milestone_key = format!("M{milestone_id}");

        let milestone_ref = match index.milestones.get(&milestone_key) {
            Some(r) => r,
            None => return Ok(default_milestone_vars(milestone_id)),
        };

        let milestone_path = project_root
            .join(".aegis")
            .join("designs")
            .join("roadmap")
            .join(&milestone_ref.path);

        let milestone_str =
            std::fs::read_to_string(&milestone_path).map_err(|e| DesignError::Io {
                path: milestone_path.to_string_lossy().into_owned(),
                reason: e.to_string(),
            })?;

        let milestone: RawMilestone =
            toml::from_str(&milestone_str).map_err(|e| DesignError::Io {
                path: milestone_path.to_string_lossy().into_owned(),
                reason: e.to_string(),
            })?;

        let mut vars = HashMap::new();
        vars.insert("milestone_id".into(), milestone_id.to_string());
        vars.insert("milestone_name".into(), milestone.name.clone());
        if let Some(lld) = &milestone.lld {
            let lld_abs = project_root
                .join(".aegis")
                .join("designs")
                .join(lld);
            vars.insert("lld_path".into(), lld_abs.to_string_lossy().into_owned());
        }
        Ok(vars)
    }
}

fn default_milestone_vars(milestone_id: u32) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("milestone_id".into(), milestone_id.to_string());
    vars
}

// Minimal deserialization types — we only need the fields BootstrapContext reads.

#[derive(Deserialize)]
struct RawIndex {
    project: RawProjectMeta,
    milestones: HashMap<String, RawMilestoneRef>,
}

#[derive(Deserialize)]
struct RawProjectMeta {
    current_milestone: u32,
}

#[derive(Deserialize)]
struct RawMilestoneRef {
    path: String,
}

#[derive(Deserialize)]
struct RawMilestone {
    name: String,
    lld: Option<String>,
}
