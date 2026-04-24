use crate::model::{Milestone, ProjectIndex, TaskflowStatus};
use crate::TaskflowEngine;
use aegis_core::lock::LockedFile;
use aegis_core::{Result, TaskStatus};
use uuid::Uuid;

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
        let mut lock = LockedFile::open_shared(&index_path)?;
        lock.read_toml()
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
        let mut lock = LockedFile::open_shared(&m_path)?;
        lock.read_toml()
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
            
            let mut lock = LockedFile::open_exclusive(&m_path)?;
            let mut milestone: Milestone = lock.read_toml()?;
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
                lock.write_toml_atomic(&milestone)?;
            }
        }

        Ok(report)
    }

    pub fn create_milestone(&self, id: &str, name: &str, lld: Option<&str>) -> Result<()> {
        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        
        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut index: ProjectIndex = index_lock.read_toml()?;

        let milestone_id_num: u32 = id.parse().map_err(|_| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone_id".into(),
                reason: "Milestone ID must be a number".into(),
            }
        })?;

        let filename = format!("M{}.toml", id);
        let rel_path = format!("milestones/{}", filename);
        let full_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&rel_path);

        if full_path.exists() {
            return Err(aegis_core::error::AegisError::ConfigValidation {
                field: "milestone_id".into(),
                reason: format!("Milestone file {} already exists", rel_path),
            });
        }

        let milestone = Milestone {
            id: milestone_id_num,
            name: name.to_string(),
            status: "pending".to_string(),
            lld: lld.map(|s| s.to_string()),
            tasks: Vec::new(),
        };

        // Write milestone file (exclusive because it's new)
        let mut m_lock = LockedFile::open_exclusive(&full_path)?;
        m_lock.write_toml_atomic(&milestone)?;

        // Update index
        index.milestones.insert(
            format!("M{}", id),
            crate::model::MilestoneRef {
                path: rel_path,
                status: "pending".to_string(),
            },
        );

        index_lock.write_toml_atomic(&index)?;

        Ok(())
    }

    pub fn add_task(&self, milestone_id: &str, task_id: &str, task_desc: &str) -> Result<()> {
        let full_m_id = if milestone_id.starts_with('M') {
            milestone_id.to_string()
        } else {
            format!("M{}", milestone_id)
        };

        let index = self.get_status()?;
        let m_ref = index.milestones.get(&full_m_id).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone".into(),
                reason: format!("Milestone {} not found in index", full_m_id),
            }
        })?;

        let m_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&m_ref.path);

        let mut lock = LockedFile::open_exclusive(&m_path)?;
        let mut milestone: Milestone = lock.read_toml()?;
        
        // Check if task ID already exists
        if milestone.tasks.iter().any(|t| t.id == task_id) {
            return Err(aegis_core::error::AegisError::ConfigValidation {
                field: "task_id".into(),
                reason: format!("Task ID {} already exists in milestone {}", task_id, full_m_id),
            });
        }

        milestone.tasks.push(crate::model::ProjectTask {
            id: task_id.to_string(),
            uid: Uuid::new_v4(),
            task: task_desc.to_string(),
            status: TaskflowStatus::Pending,
            crate_name: None,
            notes: None,
            registry_task_id: None,
        });

        lock.write_toml_atomic(&milestone)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectMeta;
    use aegis_core::{StorageBackend, TaskCreator};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    struct TestStorage {
        root: PathBuf,
    }

    impl aegis_core::StorageBackend for TestStorage {
        fn project_root(&self) -> &Path {
            &self.root
        }
    }

    struct MockTaskRegistry {
        tasks: std::sync::Mutex<HashMap<Uuid, aegis_core::Task>>,
    }

    impl aegis_core::TaskRegistry for MockTaskRegistry {
        fn insert(&self, task: &aegis_core::Task) -> aegis_core::Result<()> {
            self.tasks.lock().unwrap().insert(task.task_id, task.clone());
            Ok(())
        }
        fn get(&self, task_id: Uuid) -> aegis_core::Result<Option<aegis_core::Task>> {
            Ok(self.tasks.lock().unwrap().get(&task_id).cloned())
        }
        fn update_status(&self, _task_id: Uuid, _status: aegis_core::TaskStatus) -> aegis_core::Result<()> { Ok(()) }
        fn assign(&self, _task_id: Uuid, _agent_id: Uuid) -> aegis_core::Result<()> { Ok(()) }
        fn complete(&self, _task_id: Uuid, _receipt_path: Option<PathBuf>) -> aegis_core::Result<()> { Ok(()) }
        fn list_pending(&self) -> aegis_core::Result<Vec<aegis_core::Task>> { Ok(vec![]) }
        fn list_all(&self) -> aegis_core::Result<Vec<aegis_core::Task>> {
            Ok(self.tasks.lock().unwrap().values().cloned().collect())
        }
    }

    fn setup_engine() -> (TempDir, TaskflowEngine) {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(TestStorage { root: tmp.path().to_path_buf() });
        let registry = Arc::new(MockTaskRegistry { tasks: std::sync::Mutex::new(HashMap::new()) });

        // Bootstrap minimal index
        let roadmap_dir = storage.designs_dir().join("roadmap");
        std::fs::create_dir_all(&roadmap_dir).unwrap();
        std::fs::create_dir_all(roadmap_dir.join("milestones")).unwrap();
        std::fs::create_dir_all(storage.state_dir()).unwrap();
        
        // Initialize blank taskflow links
        std::fs::write(storage.taskflow_path(), "{}").unwrap();
        
        let index = ProjectIndex {
            project: ProjectMeta { name: "Test".to_string(), current_milestone: 1 },
            milestones: HashMap::new(),
        };
        std::fs::write(roadmap_dir.join("index.toml"), toml::to_string(&index).unwrap()).unwrap();

        (tmp, TaskflowEngine::new(storage, registry))
    }

    #[test]
    fn test_create_milestone() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("10", "Initial", None).unwrap();

        let index = engine.get_status().unwrap();
        assert!(index.milestones.contains_key("M10"));
        
        let m = engine.get_milestone("M10").unwrap();
        assert_eq!(m.name, "Initial");
        assert_eq!(m.id, 10);
    }

    #[test]
    fn test_add_task() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("10", "Initial", None).unwrap();
        engine.add_task("M10", "10.1", "First task").unwrap();

        let m = engine.get_milestone("M10").unwrap();
        assert_eq!(m.tasks.len(), 1);
        assert_eq!(m.tasks[0].id, "10.1");
        assert_eq!(m.tasks[0].task, "First task");
    }

    #[test]
    fn test_sync_updates_status() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("1", "M1", None).unwrap();
        engine.add_task("M1", "1.1", "Task 1").unwrap();

        let task_uuid = Uuid::new_v4();
        engine.registry().insert(&aegis_core::Task {
            task_id: task_uuid,
            description: "Task 1".to_string(),
            status: aegis_core::TaskStatus::Complete,
            assigned_agent_id: None,
            created_by: TaskCreator::System,
            created_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            receipt_path: None,
        }).unwrap();

        engine.links().assign("1.1".to_string(), task_uuid).unwrap();
        
        let report = engine.sync().unwrap();
        assert_eq!(report.updated_tasks.len(), 1);
        assert_eq!(report.updated_tasks[0], "1.1");

        let m = engine.get_milestone("M1").unwrap();
        assert_eq!(m.tasks[0].status, crate::model::TaskflowStatus::Done);
        assert_eq!(m.tasks[0].registry_task_id, Some(task_uuid));
    }
}
