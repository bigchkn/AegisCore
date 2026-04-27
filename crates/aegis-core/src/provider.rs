use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: String,
    pub binary: String,
    pub extra_args: Vec<String>,
    pub resume_flag: Option<String>,
    pub model: Option<String>,
    /// How long to wait after injecting the launch command before injecting the initial prompt.
    /// Gives the CLI time to start its interactive TUI before receiving input.
    pub startup_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SessionRef {
    pub provider: String,
    /// Specific session ID to resume. `None` resumes the most recent session for the provider
    /// (passed as the flag alone, without an argument — e.g. `--resume` with no session ID).
    pub session_id: Option<String>,
    pub checkpoint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FailoverContext {
    pub agent_id: Uuid,
    pub task_id: Option<Uuid>,
    pub previous_provider: String,
    /// Last N lines from the Flight Recorder log.
    pub terminal_context: String,
    pub task_description: Option<String>,
    pub worktree_path: PathBuf,
    pub role: String,
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn config(&self) -> &ProviderConfig;

    /// Build the Command to launch this provider in the given worktree.
    /// `model_override` takes precedence over any model set in the provider config.
    fn spawn_command(
        &self,
        worktree: &Path,
        session: Option<&SessionRef>,
        model_override: Option<&str>,
    ) -> Command;

    /// Arguments to append for resuming a session (used by providers that resume via CLI flags).
    fn resume_args(&self, session: &SessionRef) -> Vec<String>;

    /// Shell command string to inject into the pane to trigger a context export.
    /// Returns None if the provider has no export mechanism.
    fn export_context_command(&self) -> Option<&str>;

    fn is_rate_limit_error(&self, line: &str) -> bool;
    fn is_auth_error(&self, line: &str) -> bool;
    /// User-defined task-complete patterns are handled by the Watchdog config;
    /// providers may optionally detect their own completion signals here.
    fn is_task_complete(&self, line: &str) -> bool;

    /// Generate the recovery prompt to inject into the receiving provider at failover.
    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String;
}
