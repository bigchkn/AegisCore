use crate::handoff::render_handoff_prompt;
use crate::manifest::{ProviderDefinition, ResumeMechanism};
use aegis_core::provider::{FailoverContext, Provider, ProviderConfig, SessionRef};
use std::path::Path;
use std::process::Command;

pub struct GenericProvider {
    pub definition: ProviderDefinition,
    pub user_config: ProviderConfig,
}

impl GenericProvider {
    pub fn new(definition: ProviderDefinition, user_config: ProviderConfig) -> Self {
        Self {
            definition,
            user_config,
        }
    }
}

impl Provider for GenericProvider {
    fn name(&self) -> &str {
        &self.user_config.name
    }

    fn config(&self) -> &ProviderConfig {
        &self.user_config
    }

    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>, model_override: Option<&str>) -> Command {
        let mut cmd = Command::new(&self.user_config.binary);
        cmd.current_dir(worktree);

        if let Some(s) = session {
            if self.definition.resume_mechanism == ResumeMechanism::Subcommand {
                cmd.args(self.resume_args(s));
            }
        }

        // User extra_args before framework flags so they can't accidentally override unattended mode
        cmd.args(&self.user_config.extra_args);

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

        // Model: per-call override wins over provider-level config
        let model = model_override.or(self.user_config.model.as_deref());
        if let (Some(flag), Some(model)) = (&self.definition.model_flag, model) {
            cmd.arg(flag).arg(model);
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
        } else if self.definition.resume_mechanism == ResumeMechanism::Subcommand {
            if let Some(command) = &self.definition.resume_command {
                args.extend(
                    command
                        .replace("{session_id}", &session.session_id)
                        .split_whitespace()
                        .map(str::to_owned),
                );
            }
        }
        args
    }

    fn export_context_command(&self) -> Option<&str> {
        self.definition.export_command.as_deref()
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.definition
            .error_patterns
            .rate_limit
            .iter()
            .any(|p| l.contains(p))
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.definition
            .error_patterns
            .auth
            .iter()
            .any(|p| l.contains(p))
    }

    fn is_task_complete(&self, _line: &str) -> bool {
        false
    }

    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String {
        render_handoff_prompt(ctx)
    }
}
