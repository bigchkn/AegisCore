use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum TaskStatus {
    Queued,
    Active,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum TaskCreator {
    Agent(Uuid),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Task {
    pub task_id: Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_agent_id: Option<Uuid>,
    pub created_by: TaskCreator,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub receipt_path: Option<PathBuf>,
}

pub trait TaskQueue: Send + Sync {
    /// Enqueue a new task; returns the assigned task_id.
    fn enqueue(&self, description: &str, created_by: TaskCreator) -> Result<Uuid>;
    /// Atomically claim the next queued task for an agent. Returns None if empty.
    fn claim_next(&self, agent_id: Uuid) -> Result<Option<Task>>;
    fn pending_count(&self) -> Result<usize>;
}

pub trait TaskRegistry: Send + Sync {
    fn insert(&self, task: &Task) -> Result<()>;
    fn get(&self, task_id: Uuid) -> Result<Option<Task>>;
    fn update_status(&self, task_id: Uuid, status: TaskStatus) -> Result<()>;
    fn assign(&self, task_id: Uuid, agent_id: Uuid) -> Result<()>;
    fn complete(&self, task_id: Uuid, receipt_path: Option<PathBuf>) -> Result<()>;
    fn list_pending(&self) -> Result<Vec<Task>>;
    fn list_all(&self) -> Result<Vec<Task>>;
}
