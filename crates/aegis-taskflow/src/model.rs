use serde::{Deserialize, Deserializer, Serialize};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    #[default]
    Feature,
    Bug,
    Maintenance,
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
    pub backlog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneRef {
    pub name: String,
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    #[serde(deserialize_with = "deserialize_milestone_id")]
    pub id: u32,
    pub name: String,
    pub status: String,
    pub lld: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub tasks: Vec<ProjectTask>,
}

fn deserialize_milestone_id<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    struct MilestoneIdVisitor;

    impl<'de> serde::de::Visitor<'de> for MilestoneIdVisitor {
        type Value = u32;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a milestone id as a number or an M-prefixed string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u32::try_from(value)
                .map_err(|_| E::custom(format!("milestone id {value} is too large")))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u32::try_from(value).map_err(|_| E::custom(format!("milestone id {value} is invalid")))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let trimmed = value.trim();
            let numeric = trimmed.strip_prefix('M').unwrap_or(trimmed);
            numeric
                .parse::<u32>()
                .map_err(|_| E::custom(format!("milestone id {value} is invalid")))
        }
    }

    deserializer.deserialize_any(MilestoneIdVisitor)
}

#[cfg(test)]
mod tests {
    use super::Milestone;

    #[test]
    fn milestone_id_accepts_legacy_string_prefix() {
        let milestone: Milestone = toml::from_str(
            r#"
id = "M26"
name = "Human Clarification UI"
status = "pending"
lld = "lld/human-clarification.md"
tasks = []
"#,
        )
        .unwrap();

        assert_eq!(milestone.id, 26);
    }

    #[test]
    fn milestone_id_accepts_numeric_value() {
        let milestone: Milestone = toml::from_str(
            r#"
id = 27
name = "Agent Attach & Live Inspection"
status = "pending"
tasks = []
"#,
        )
        .unwrap();

        assert_eq!(milestone.id, 27);
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub task: String,
    #[serde(default)]
    pub task_type: TaskType,
    #[serde(default)]
    pub status: Option<TaskflowStatus>,
    #[serde(default)]
    pub crate_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskPatch {
    pub id: Option<String>,
    pub task: Option<String>,
    #[serde(default)]
    pub task_type: Option<TaskType>,
    #[serde(default)]
    pub status: Option<TaskflowStatus>,
    #[serde(default)]
    pub crate_name: Option<Option<String>>,
    #[serde(default)]
    pub notes: Option<Option<String>>,
    pub target_milestone_id: Option<String>,
}
