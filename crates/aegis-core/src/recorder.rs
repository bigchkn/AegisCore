use chrono::{DateTime, Utc};
use std::path::PathBuf;
use uuid::Uuid;

use crate::agent::Agent;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct LogQuery {
    pub agent_id: Uuid,
    /// Number of trailing lines to return. None = entire log.
    pub last_n_lines: Option<usize>,
    /// Only return lines after this timestamp (best-effort; log lines are raw terminal output).
    pub since: Option<DateTime<Utc>>,
    /// Hint to the caller that streaming is intended; recorder returns the current snapshot.
    pub follow: bool,
}

pub trait Recorder: Send + Sync {
    /// Attach a flight recorder to the agent's tmux pane. Called at spawn time.
    fn attach(&self, agent: &Agent) -> Result<()>;

    /// Detach the recorder from the pane. Called before the pane is closed.
    fn detach(&self, agent_id: Uuid) -> Result<()>;

    /// Move the live log to the archive directory. Returns the archive path.
    fn archive(&self, agent_id: Uuid) -> Result<PathBuf>;

    /// Return lines from the agent's log matching the query.
    fn query(&self, query: &LogQuery) -> Result<Vec<String>>;

    /// Canonical path for the live session log of the given agent.
    fn log_path(&self, agent_id: Uuid) -> PathBuf;
}
