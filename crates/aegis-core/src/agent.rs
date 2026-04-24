use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Bastion,
    Splinter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Queued,
    Starting,
    Active,
    Paused,
    Cooling,
    Reporting,
    Terminated,
    Failed,
}

impl AgentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminated | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: Uuid,
    pub name: String,
    pub kind: AgentKind,
    pub status: AgentStatus,
    pub role: String,
    pub parent_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub tmux_session: String,
    pub tmux_window: u32,
    pub tmux_pane: String,
    pub worktree_path: PathBuf,
    pub cli_provider: String,
    pub fallback_cascade: Vec<String>,
    pub sandbox_profile: PathBuf,
    pub log_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminated_at: Option<DateTime<Utc>>,
}

impl Agent {
    pub fn tmux_target(&self) -> String {
        format!("{}:{}.{}", self.tmux_session, self.tmux_window, self.tmux_pane)
    }
}

pub trait AgentHandle: Send + Sync {
    fn agent_id(&self) -> Uuid;
    fn tmux_target(&self) -> String;
    fn worktree_path(&self) -> &Path;
    fn is_alive(&self) -> bool;
}

pub trait AgentRegistry: Send + Sync {
    fn insert(&self, agent: &Agent) -> Result<()>;
    fn get(&self, agent_id: Uuid) -> Result<Option<Agent>>;
    fn update(&self, agent: &Agent) -> Result<()>;
    fn update_status(&self, agent_id: Uuid, status: AgentStatus) -> Result<()>;
    fn update_provider(&self, agent_id: Uuid, provider: &str) -> Result<()>;
    fn list_active(&self) -> Result<Vec<Agent>>;
    fn list_by_role(&self, role: &str) -> Result<Vec<Agent>>;
    fn list_all(&self) -> Result<Vec<Agent>>;
    fn archive(&self, agent_id: Uuid) -> Result<()>;
}
