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
}

#[derive(Debug, Clone)]
pub struct SessionRef {
    pub provider: String,
    pub session_id: String,
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
    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command;

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
