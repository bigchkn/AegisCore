use std::path::Path;
use std::process::Command;
use aegis_core::provider::{Provider, ProviderConfig, SessionRef, FailoverContext};
use crate::manifest::ProviderDefinition;
use crate::handoff::render_handoff_prompt;

pub struct OllamaProvider {
    pub manifest: ProviderDefinition,
    pub user_config: ProviderConfig,
}

impl OllamaProvider {
    pub fn new(manifest: ProviderDefinition, user_config: ProviderConfig) -> Self {
        Self { manifest, user_config }
    }
}

impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        &self.user_config.name
    }

    fn config(&self) -> &ProviderConfig {
        &self.user_config
    }

    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let bin = &self.user_config.binary;
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);
        
        let model = self.user_config.model.as_deref().unwrap_or("gemma3");
        cmd.args(["run", model]);
        
        cmd.args(&self.manifest.auto_approve_flags);
        cmd.args(&self.manifest.non_interactive_flags);
        cmd
    }

    fn resume_args(&self, _session: &SessionRef) -> Vec<String> {
        Vec::new()
    }

    fn export_context_command(&self) -> Option<&str> {
        None
    }

    fn is_rate_limit_error(&self, _line: &str) -> bool {
        false
    }

    fn is_auth_error(&self, _line: &str) -> bool {
        false
    }

    fn is_task_complete(&self, _line: &str) -> bool {
        false
    }

    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String {
        render_handoff_prompt(ctx)
    }
}
