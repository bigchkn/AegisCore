use std::path::Path;
use std::process::Command;
use aegis_core::provider::{Provider, ProviderConfig, SessionRef, FailoverContext};
use crate::manifest::ProviderDefinition;
use crate::handoff::render_handoff_prompt;

pub struct ClaudeProvider {
    pub manifest: ProviderDefinition,
    pub user_config: ProviderConfig,
}

impl ClaudeProvider {
    pub fn new(manifest: ProviderDefinition, user_config: ProviderConfig) -> Self {
        Self { manifest, user_config }
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        &self.user_config.name
    }

    fn config(&self) -> &ProviderConfig {
        &self.user_config
    }

    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command {
        let bin = &self.user_config.binary;
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);

        // Always use manifest flags for unattended/non-interactive
        cmd.args(&self.manifest.auto_approve_flags);
        cmd.args(&self.manifest.non_interactive_flags);

        if let Some(s) = session {
            if let Some(flag) = &self.manifest.resume_flag {
                cmd.arg(flag).arg(&s.session_id);
            }
        }

        cmd
    }

    fn resume_args(&self, session: &SessionRef) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(flag) = &self.manifest.resume_flag {
            args.push(flag.clone());
            args.push(session.session_id.clone());
        }
        args
    }

    fn export_context_command(&self) -> Option<&str> {
        Some("/export")
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.rate_limit.iter().any(|p| l.contains(p))
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.auth.iter().any(|p| l.contains(p))
    }

    fn is_task_complete(&self, _line: &str) -> bool {
        false
    }

    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String {
        render_handoff_prompt(ctx)
    }
}
