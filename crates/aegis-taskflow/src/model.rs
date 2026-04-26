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

impl TaskflowStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "lld-in-progress" => Some(Self::LldInProgress),
            "lld-done" => Some(Self::LldDone),
            "in-progress" => Some(Self::InProgress),
            "done" => Some(Self::Done),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::LldInProgress => "lld-in-progress",
            Self::LldDone => "lld-done",
            Self::InProgress => "in-progress",
            Self::Done => "done",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    Feature,
    Bug,
    Maintenance,
}

impl Default for TaskType {
    fn default() -> Self {
        Self::Feature
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub project: ProjectMeta,
    pub milestones: HashMap<String, MilestoneRef>,
    pub backlog: Option<String>,
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
    #[serde(default = "Uuid::new_v4")]
    pub uid: Uuid,
    pub task: String,
    #[serde(default)]
    pub task_type: TaskType,
    pub status: TaskflowStatus,
    pub crate_name: Option<String>,
    pub notes: Option<String>,
    pub registry_task_id: Option<Uuid>,
}
