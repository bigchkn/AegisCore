use uuid::Uuid;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use aegis_core::task::{Task, TaskQueue, TaskRegistry, TaskStatus, TaskCreator};
use aegis_core::error::{Result, AegisError};
use crate::registry::{FileRegistry, LockedFile};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TaskStore {
    pub version: u32,
    pub tasks: Vec<Task>,
}

impl TaskQueue for FileRegistry {
    fn enqueue(&self, description: &str, created_by: TaskCreator) -> Result<Uuid> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        
        let task = Task {
            task_id: Uuid::new_v4(),
            description: description.to_string(),
            status: TaskStatus::Queued,
            assigned_agent_id: None,
            created_by,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        
        let id = task.task_id;
        store.tasks.push(task);
        file.write_json_atomic(&store)?;
        Ok(id)
    }

    fn claim_next(&self, agent_id: Uuid) -> Result<Option<Task>> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        
        if let Some(task) = store.tasks.iter_mut().find(|t| t.status == TaskStatus::Queued) {
            task.status = TaskStatus::Active;
            task.assigned_agent_id = Some(agent_id);
            let claimed = task.clone();
            file.write_json_atomic(&store)?;
            Ok(Some(claimed))
        } else {
            Ok(None)
        }
    }

    fn pending_count(&self) -> Result<usize> {
        let mut file = LockedFile::open_shared(&self.storage.tasks_path())?;
        let store: TaskStore = file.read_json()?;
        Ok(store.tasks.iter().filter(|t| t.status == TaskStatus::Queued).count())
    }
}

impl TaskRegistry for FileRegistry {
    fn insert(&self, task: &Task) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        store.tasks.push(task.clone());
        file.write_json_atomic(&store)
    }

    fn get(&self, task_id: Uuid) -> Result<Option<Task>> {
        let mut file = LockedFile::open_shared(&self.storage.tasks_path())?;
        let store: TaskStore = file.read_json()?;
        Ok(store.tasks.iter().find(|t| t.task_id == task_id).cloned())
    }

    fn update_status(&self, task_id: Uuid, status: TaskStatus) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        
        if let Some(task) = store.tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.status = status;
            if task.status == TaskStatus::Complete || task.status == TaskStatus::Failed {
                task.completed_at = Some(Utc::now());
            }
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::TaskNotFound { task_id })
        }
    }

    fn assign(&self, task_id: Uuid, agent_id: Uuid) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        
        if let Some(task) = store.tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.assigned_agent_id = Some(agent_id);
            task.status = TaskStatus::Active;
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::TaskNotFound { task_id })
        }
    }

    fn complete(&self, task_id: Uuid, receipt_path: Option<std::path::PathBuf>) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.tasks_path())?;
        let mut store: TaskStore = file.read_json()?;
        
        if let Some(task) = store.tasks.iter_mut().find(|t| t.task_id == task_id) {
            task.status = TaskStatus::Complete;
            task.completed_at = Some(Utc::now());
            task.receipt_path = receipt_path;
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::TaskNotFound { task_id })
        }
    }

    fn list_pending(&self) -> Result<Vec<Task>> {
        let mut file = LockedFile::open_shared(&self.storage.tasks_path())?;
        let store: TaskStore = file.read_json()?;
        Ok(store.tasks.iter().filter(|t| t.status == TaskStatus::Queued).cloned().collect())
    }

    fn list_all(&self) -> Result<Vec<Task>> {
        let mut file = LockedFile::open_shared(&self.storage.tasks_path())?;
        let store: TaskStore = file.read_json()?;
        Ok(store.tasks.clone())
    }
}
