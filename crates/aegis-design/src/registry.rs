use crate::error::{DesignError, Result};
use crate::template::Template;
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateLayer {
    BuiltIn,
    Global,
    Project,
}

impl std::fmt::Display for TemplateLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn => write!(f, "built-in"),
            Self::Global => write!(f, "global"),
            Self::Project => write!(f, "project"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedTemplate {
    pub name: String,
    pub template: Template,
    pub layer: TemplateLayer,
}

pub struct TemplateRegistry {
    templates: HashMap<String, ResolvedTemplate>,
}

impl TemplateRegistry {
    /// Load templates from all three layers in priority order.
    /// Project-local shadows global, global shadows built-in.
    pub fn load(project_root: &Path) -> Self {
        let mut templates: HashMap<String, ResolvedTemplate> = HashMap::new();

        // Layer 1: built-ins (lowest priority)
        for &(name, toml, system_prompt, startup) in builtin_template_sources() {
            match Template::from_parts(
                name,
                toml,
                system_prompt.to_owned(),
                startup.map(str::to_owned),
            ) {
                Ok(t) => {
                    templates.insert(
                        name.to_owned(),
                        ResolvedTemplate {
                            name: name.to_owned(),
                            template: t,
                            layer: TemplateLayer::BuiltIn,
                        },
                    );
                }
                Err(e) => tracing::warn!("failed to load built-in template '{name}': {e}"),
            }
        }

        // Layer 2: global user templates (~/.aegis/templates/)
        if let Some(home) = dirs_next(project_root) {
            let global_dir = home.join(".aegis").join("templates");
            load_from_dir(&global_dir, TemplateLayer::Global, &mut templates);
        }

        // Layer 3: project-local templates (.aegis/templates/)
        let project_dir = project_root.join(".aegis").join("templates");
        load_from_dir(&project_dir, TemplateLayer::Project, &mut templates);

        Self { templates }
    }

    pub fn get(&self, name: &str) -> Result<&ResolvedTemplate> {
        self.templates
            .get(name)
            .ok_or_else(|| DesignError::TemplateNotFound {
                name: name.to_owned(),
            })
    }

    pub fn list(&self) -> Vec<&ResolvedTemplate> {
        let mut items: Vec<_> = self.templates.values().collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
    }
}

fn load_from_dir(dir: &Path, layer: TemplateLayer, out: &mut HashMap<String, ResolvedTemplate>) {
    if !dir.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("cannot read template dir {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        let toml_path = path.join("template.toml");
        let prompt_path = path.join("system_prompt.md");
        if !toml_path.exists() || !prompt_path.exists() {
            debug!(
                "skipping {}: missing template.toml or system_prompt.md",
                path.display()
            );
            continue;
        }
        let toml_content = match std::fs::read_to_string(&toml_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("cannot read {}: {e}", toml_path.display());
                continue;
            }
        };
        let system_prompt = match std::fs::read_to_string(&prompt_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("cannot read {}: {e}", prompt_path.display());
                continue;
            }
        };
        let startup_path = path.join("startup.md");
        let startup = if startup_path.exists() {
            match std::fs::read_to_string(&startup_path) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("cannot read {}: {e}", startup_path.display());
                    None
                }
            }
        } else {
            None
        };
        match Template::from_parts(&name, &toml_content, system_prompt, startup) {
            Ok(t) => {
                out.insert(
                    name.clone(),
                    ResolvedTemplate {
                        name,
                        template: t,
                        layer: layer.clone(),
                    },
                );
            }
            Err(e) => tracing::warn!("failed to load template '{name}': {e}"),
        }
    }
}

/// Return the user home directory by walking up from the project root if needed.
/// Uses the HOME env var, which is always set on macOS/Linux.
fn dirs_next(_project_root: &Path) -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

/// Built-in templates embedded at compile time.
fn builtin_template_sources() -> &'static [(
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
)] {
    &[
        (
            "taskflow-bastion",
            include_str!("builtin/taskflow-bastion/template.toml"),
            include_str!("builtin/taskflow-bastion/system_prompt.md"),
            Some(include_str!("builtin/taskflow-bastion/startup.md")),
        ),
        (
            "taskflow-implementer",
            include_str!("builtin/taskflow-implementer/template.toml"),
            include_str!("builtin/taskflow-implementer/system_prompt.md"),
            Some(include_str!("builtin/taskflow-implementer/startup.md")),
        ),
        (
            "taskflow-designer",
            include_str!("builtin/taskflow-designer/template.toml"),
            include_str!("builtin/taskflow-designer/system_prompt.md"),
            Some(include_str!("builtin/taskflow-designer/startup.md")),
        ),
    ]
}
