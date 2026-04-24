use std::path::Path;
use std::process::Command;
use aegis_core::provider::{Provider, ProviderConfig, SessionRef, FailoverContext};
use crate::manifest::{ProviderDefinition, ResumeMechanism};
use crate::handoff::render_handoff_prompt;

pub struct GenericProvider {
    pub definition: ProviderDefinition,
    pub user_config: ProviderConfig,
}

impl GenericProvider {
    pub fn new(definition: ProviderDefinition, user_config: ProviderConfig) -> Self {
        Self { definition, user_config }
    }
}

impl Provider for GenericProvider {
    fn name(&self) -> &str {
        &self.user_config.name
    }

    fn config(&self) -> &ProviderConfig {
        &self.user_config
    }

    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command {
        let mut cmd = Command::new(&self.user_config.binary);
        cmd.current_dir(worktree);

        // Standard unattended flags
        cmd.args(&self.definition.auto_approve_flags);
        cmd.args(&self.definition.non_interactive_flags);

        // CLI-level resume if applicable
        if let Some(s) = session {
            if self.definition.resume_mechanism == ResumeMechanism::CliFlag {
                if let Some(flag) = &self.definition.resume_flag {
                    cmd.arg(flag).arg(&s.session_id);
                }
            }
        }

        cmd
    }

    fn resume_args(&self, session: &SessionRef) -> Vec<String> {
        let mut args = Vec::new();
        if self.definition.resume_mechanism == ResumeMechanism::CliFlag {
            if let Some(flag) = &self.definition.resume_flag {
                args.push(flag.clone());
                args.push(session.session_id.clone());
            }
        }
        args
    }

    fn export_context_command(&self) -> Option<&str> {
        self.definition.export_command.as_deref()
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.definition.error_patterns.rate_limit.iter().any(|p| l.contains(p))
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.definition.error_patterns.auth.iter().any(|p| l.contains(p))
    }

    fn is_task_complete(&self, _line: &str) -> bool {
        false
    }

    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String {
        render_handoff_prompt(ctx)
    }
}
