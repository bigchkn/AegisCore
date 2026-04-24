use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskflowStatus {
    Pending,
    LldInProgress,
    LldDone,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub project: ProjectMeta,
    pub milestones: HashMap<String, MilestoneRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    pub current_milestone: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneRef {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: u32,
    pub name: String,
    pub status: String,
    pub lld: Option<String>,
    pub tasks: Vec<ProjectTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTask {
    pub id: String,
    pub uid: Uuid,
    pub task: String,
    pub status: TaskflowStatus,
    pub crate_name: Option<String>,
    pub notes: Option<String>,
    pub registry_task_id: Option<Uuid>,
}
