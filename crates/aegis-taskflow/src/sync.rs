use crate::model::{Milestone, ProjectIndex, TaskflowStatus};
use crate::TaskflowEngine;
use aegis_core::{Result, TaskStatus};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub updated_tasks: Vec<String>, // roadmap_ids
}

impl TaskflowEngine {
    pub fn get_status(&self) -> Result<ProjectIndex> {
        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        crate::parser::parse_index(&index_path)
    }

    pub fn get_milestone(&self, milestone_id: &str) -> Result<Milestone> {
        let index = self.get_status()?;
        let m_ref = index.milestones.get(milestone_id).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone".into(),
                reason: format!("Milestone {} not found in index", milestone_id),
            }
        })?;

        let m_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&m_ref.path);
        crate::parser::parse_milestone(&m_path)
    }

    pub fn sync(&self) -> Result<SyncReport> {
        let mut report = SyncReport {
            updated_tasks: Vec::new(),
        };
        let links = self.links().list_all()?;
        let index = self.get_status()?;

        // For each milestone in index
        for (_m_id, m_ref) in index.milestones {
            let m_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join(&m_ref.path);
            let mut milestone = crate::parser::parse_milestone(&m_path)?;
            let mut modified = false;

            for task in &mut milestone.tasks {
                if let Some(registry_id) = links.get(&task.id) {
                    if let Some(registry_task) = self.registry().get(*registry_id)? {
                        let new_status = match registry_task.status {
                            TaskStatus::Complete => TaskflowStatus::Done,
                            TaskStatus::Active => TaskflowStatus::InProgress,
                            TaskStatus::Failed => TaskflowStatus::Blocked,
                            TaskStatus::Queued => TaskflowStatus::InProgress,
                        };

                        if task.status != new_status {
                            task.status = new_status;
                            task.registry_task_id = Some(*registry_id);
                            report.updated_tasks.push(task.id.clone());
                            modified = true;
                        }
                    }
                }
            }

            if modified {
                // Write back the updated TOML
                let content = toml::to_string_pretty(&milestone).map_err(|e| {
                    aegis_core::error::AegisError::ConfigSerializationError {
                        path: m_path.clone(),
                        source: e,
                    }
                })?;
                std::fs::write(&m_path, content).map_err(|e| {
                    aegis_core::error::AegisError::StorageIo {
                        path: m_path.clone(),
                        source: e,
                    }
                })?;
            }
        }

        Ok(report)
    }
}
