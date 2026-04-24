use std::path::{Path, PathBuf};
use std::collections::HashMap;
use uuid::Uuid;
use aegis_core::{Result, AegisError};

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("prompts/templates/system_default.md");
const DEFAULT_RECOVERY_PROMPT: &str = include_str!("prompts/templates/recovery_default.md");
const DEFAULT_RESUME_PROMPT: &str = include_str!("prompts/templates/resume_default.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptType {
    System,
    Task,
    Recovery,
    Resume,
}

#[derive(Debug, Clone)]
pub struct PromptContext {
    pub agent_id: Uuid,
    pub role: String,
    pub task_id: Option<Uuid>,
    pub task_description: Option<String>,
    pub context_snippet: Option<String>,
    pub worktree_path: PathBuf,
    pub previous_cli: Option<String>,
}

pub struct PromptManager {
    project_root: PathBuf,
}

impl PromptManager {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Resolve and render a prompt.
    /// 
    /// Resolution order for System prompt:
    /// 1. role_override (explicit path from config)
    /// 2. .aegis/prompts/system/<role>.md
    /// 3. Built-in default
    pub fn resolve_prompt(
        &self,
        template_type: PromptType,
        context: &PromptContext,
        role_override: Option<&Path>,
    ) -> Result<String> {
        let raw_template = match template_type {
            PromptType::System => self.load_system_template(&context.role, role_override)?,
            PromptType::Task => self.load_task_template(context.role.as_str())?, // Placeholder for now
            PromptType::Recovery => self.load_handoff_template("recovery")?,
            PromptType::Resume => self.load_handoff_template("resume")?,
        };

        Ok(self.render(&raw_template, context))
    }

    fn load_system_template(&self, role: &str, role_override: Option<&Path>) -> Result<String> {
        // 1. Explicit override from config
        if let Some(path) = role_override {
            let full_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                self.project_root.join(path)
            };
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                return Ok(content);
            }
        }

        // 2. Project-specific role file: .aegis/prompts/system/<role>.md
        let project_role_path = self.project_root.join(".aegis/prompts/system").join(format!("{}.md", role));
        if let Ok(content) = std::fs::read_to_string(&project_role_path) {
            return Ok(content);
        }

        // 3. Built-in default
        Ok(DEFAULT_SYSTEM_PROMPT.to_string())
    }

    fn load_task_template(&self, _task_type: &str) -> Result<String> {
        // For now, task prompts are often just the task description itself, 
        // but we can support templates in .aegis/prompts/task/
        Ok("{{task}}".to_string())
    }

    fn load_handoff_template(&self, name: &str) -> Result<String> {
        let path = self.project_root.join(".aegis/prompts/handoff").join(format!("{}.md", name));
        if let Ok(content) = std::fs::read_to_string(&path) {
            return Ok(content);
        }

        match name {
            "recovery" => Ok(DEFAULT_RECOVERY_PROMPT.to_string()),
            "resume" => Ok(DEFAULT_RESUME_PROMPT.to_string()),
            _ => Err(AegisError::Config { 
                field: "prompt_template".into(), 
                reason: format!("Unknown handoff template: {}", name) 
            }),
        }
    }

    fn render(&self, template: &str, context: &PromptContext) -> String {
        let mut vars = HashMap::new();
        vars.insert("agent_id", context.agent_id.to_string());
        vars.insert("role", context.role.clone());
        vars.insert("task_id", context.task_id.map(|id| id.to_string()).unwrap_or_default());
        vars.insert("task", context.task_description.clone().unwrap_or_default());
        vars.insert("context", context.context_snippet.clone().unwrap_or_default());
        vars.insert("worktree_path", context.worktree_path.to_string_lossy().to_string());
        vars.insert("previous_cli", context.previous_cli.clone().unwrap_or_default());

        let mut rendered = template.to_string();
        for (key, value) in vars {
            let pattern = format!("{{{{{}}}}}", key);
            rendered = rendered.replace(&pattern, &value);
        }
        rendered
    }

    /// Scaffold default prompts into the project directory.
    pub fn scaffold_defaults(&self) -> Result<()> {
        let system_dir = self.project_root.join(".aegis/prompts/system");
        let handoff_dir = self.project_root.join(".aegis/prompts/handoff");

        std::fs::create_dir_all(&system_dir).map_err(|e| AegisError::StorageIo { path: system_dir.clone(), source: e })?;
        std::fs::create_dir_all(&handoff_dir).map_err(|e| AegisError::StorageIo { path: handoff_dir.clone(), source: e })?;

        // We don't overwrite existing files to respect user customizations
        let default_system = system_dir.join("default.md");
        if !default_system.exists() {
            std::fs::write(&default_system, DEFAULT_SYSTEM_PROMPT).map_err(|e| AegisError::StorageIo { path: default_system, source: e })?;
        }

        let recovery = handoff_dir.join("recovery.md");
        if !recovery.exists() {
            std::fs::write(&recovery, DEFAULT_RECOVERY_PROMPT).map_err(|e| AegisError::StorageIo { path: recovery, source: e })?;
        }

        let resume = handoff_dir.join("resume.md");
        if !resume.exists() {
            std::fs::write(&resume, DEFAULT_RESUME_PROMPT).map_err(|e| AegisError::StorageIo { path: resume, source: e })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_render_variables() {
        let pm = PromptManager::new(PathBuf::from("/tmp"));
        let ctx = PromptContext {
            agent_id: Uuid::nil(),
            role: "architect".into(),
            task_id: None,
            task_description: Some("build the app".into()),
            context_snippet: Some("previous log".into()),
            worktree_path: PathBuf::from("/ws"),
            previous_cli: Some("claude".into()),
        };

        let template = "Role: {{role}}, Task: {{task}}, Prev: {{previous_cli}}, Dir: {{worktree_path}}";
        let rendered = pm.render(template, &ctx);
        assert_eq!(rendered, "Role: architect, Task: build the app, Prev: claude, Dir: /ws");
    }

    #[test]
    fn test_resolve_system_default() {
        let dir = tempdir().unwrap();
        let pm = PromptManager::new(dir.path().to_path_buf());
        let ctx = PromptContext {
            agent_id: Uuid::nil(),
            role: "unknown".into(),
            task_id: None,
            task_description: None,
            context_snippet: None,
            worktree_path: PathBuf::from("/ws"),
            previous_cli: None,
        };

        let rendered = pm.resolve_prompt(PromptType::System, &ctx, None).unwrap();
        assert!(rendered.contains("role of unknown"));
        assert!(rendered.contains("workspace is located at /ws"));
    }

    #[test]
    fn test_resolve_system_file_override() {
        let dir = tempdir().unwrap();
        let prompts_dir = dir.path().join(".aegis/prompts/system");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("architect.md"), "Custom Architect Prompt: {{task}}").unwrap();

        let pm = PromptManager::new(dir.path().to_path_buf());
        let ctx = PromptContext {
            agent_id: Uuid::nil(),
            role: "architect".into(),
            task_id: None,
            task_description: Some("design it".into()),
            context_snippet: None,
            worktree_path: PathBuf::from("/ws"),
            previous_cli: None,
        };

        let rendered = pm.resolve_prompt(PromptType::System, &ctx, None).unwrap();
        assert_eq!(rendered, "Custom Architect Prompt: design it");
    }

    #[test]
    fn test_resolve_explicit_override() {
        let dir = tempdir().unwrap();
        let override_file = dir.path().join("my_prompt.md");
        std::fs::write(&override_file, "Explicit: {{role}}").unwrap();

        let pm = PromptManager::new(dir.path().to_path_buf());
        let ctx = PromptContext {
            agent_id: Uuid::nil(),
            role: "pm".into(),
            task_id: None,
            task_description: None,
            context_snippet: None,
            worktree_path: PathBuf::from("/ws"),
            previous_cli: None,
        };

        let rendered = pm.resolve_prompt(PromptType::System, &ctx, Some(&override_file)).unwrap();
        assert_eq!(rendered, "Explicit: pm");
    }

    #[test]
    fn test_scaffold_defaults() {
        let dir = tempdir().unwrap();
        let pm = PromptManager::new(dir.path().to_path_buf());
        pm.scaffold_defaults().unwrap();

        assert!(dir.path().join(".aegis/prompts/system/default.md").exists());
        assert!(dir.path().join(".aegis/prompts/handoff/recovery.md").exists());
        assert!(dir.path().join(".aegis/prompts/handoff/resume.md").exists());
    }
}
