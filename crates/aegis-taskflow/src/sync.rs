use crate::model::{Milestone, ProjectIndex, TaskDraft, TaskPatch, TaskflowStatus};
use crate::TaskflowEngine;
use aegis_core::lock::LockedFile;
use aegis_core::{Result, TaskStatus};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncReport {
    pub updated_tasks: Vec<String>, // roadmap_ids
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Backlog {
    pub tasks: Vec<crate::model::ProjectTask>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextMilestoneOutcome {
    Ready {
        milestone_id: String,
        name: String,
        task_count: usize,
        tasks: Vec<String>,
    },
    Exhausted,
    Blocked {
        waiting_on: Vec<String>,
    },
}

impl TaskflowEngine {
    fn resolve_taskflow_status(value: &str) -> Result<TaskflowStatus> {
        TaskflowStatus::parse(value).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "status".into(),
                reason: format!("Unsupported task status: {value}"),
            }
        })
    }

    fn derive_milestone_status(tasks: &[crate::model::ProjectTask]) -> TaskflowStatus {
        if tasks.is_empty() {
            return TaskflowStatus::Pending;
        }

        if tasks.iter().all(|task| task.status == TaskflowStatus::Done) {
            return TaskflowStatus::Done;
        }

        if tasks
            .iter()
            .any(|task| task.status != TaskflowStatus::Pending)
        {
            return TaskflowStatus::InProgress;
        }

        TaskflowStatus::Pending
    }

    fn normalize_milestone_id(milestone_id: &str) -> String {
        if milestone_id == "backlog" {
            return "backlog".to_string();
        }

        if milestone_id.starts_with('M') {
            milestone_id.to_string()
        } else {
            format!("M{}", milestone_id)
        }
    }

    fn task_id_conflicts(
        &self,
        desired_id: &str,
        exclude_uid: Option<Uuid>,
        skip_paths: &[PathBuf],
    ) -> Result<bool> {
        let index = self.get_status()?;
        let roadmap_dir = self.storage().designs_dir().join("roadmap");
        let backlog_path = roadmap_dir.join("backlog.toml");

        if !skip_paths.iter().any(|path| path == &backlog_path) {
            if let Ok(backlog) = self.get_backlog() {
                if backlog.tasks.iter().any(|task| {
                    task.id == desired_id && exclude_uid.map(|uid| uid != task.uid).unwrap_or(true)
                }) {
                    return Ok(true);
                }
            }
        }

        for (_key, m_ref) in index.milestones {
            let m_path = roadmap_dir.join(&m_ref.path);
            if skip_paths.iter().any(|path| path == &m_path) {
                continue;
            }
            let mut lock = LockedFile::open_shared(&m_path)?;
            let milestone: Milestone = lock.read_toml()?;
            if milestone.tasks.iter().any(|task| {
                task.id == desired_id && exclude_uid.map(|uid| uid != task.uid).unwrap_or(true)
            }) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn next_backlog_task_id(tasks: &[crate::model::ProjectTask]) -> String {
        let next = tasks
            .iter()
            .filter_map(|task| task.id.strip_prefix('B'))
            .filter_map(|suffix| suffix.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        format!("B{next}")
    }

    fn next_milestone_task_id(milestone: &Milestone) -> String {
        let prefix = format!("{}.", milestone.id);
        let next = milestone
            .tasks
            .iter()
            .filter_map(|task| task.id.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        format!("{}.{}", milestone.id, next)
    }

    fn apply_task_patch(task: &mut crate::model::ProjectTask, patch: &TaskPatch) {
        if let Some(id) = &patch.id {
            task.id = id.clone();
        }
        if let Some(desc) = &patch.task {
            task.task = desc.clone();
        }
        if let Some(task_type) = patch.task_type {
            task.task_type = task_type;
        }
        if let Some(status) = &patch.status {
            task.status = status.clone();
        }
        if let Some(crate_name) = &patch.crate_name {
            task.crate_name = crate_name.clone();
        }
        if let Some(notes) = &patch.notes {
            task.notes = notes.clone();
        }
    }

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
        if milestone_id == "backlog" {
            let backlog = self.get_backlog()?;
            return Ok(Milestone {
                id: 0,
                name: "Global Backlog".to_string(),
                status: "n/a".to_string(),
                lld: None,
                depends_on: Vec::new(),
                tasks: backlog.tasks,
            });
        }

        let index = self.get_status()?;
        let full_id =
            if milestone_id.starts_with('M') || milestone_id.chars().all(|c| c.is_numeric()) {
                if milestone_id.starts_with('M') {
                    milestone_id.to_string()
                } else {
                    format!("M{}", milestone_id)
                }
            } else {
                milestone_id.to_string()
            };

        let m_ref = index.milestones.get(&full_id).ok_or_else(|| {
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

    pub fn get_backlog(&self) -> Result<Backlog> {
        let backlog_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("backlog.toml");

        if !backlog_path.exists() {
            return Ok(Backlog { tasks: Vec::new() });
        }

        let mut lock = LockedFile::open_shared(&backlog_path)?;
        lock.read_toml()
    }

    /// Returns the next roadmap bucket the bastion should work on. Global
    /// backlog work takes priority, then milestones use greedy topological
    /// ordering: lowest-ID ready milestone whose dependencies are all done.
    pub fn next_milestone(&self) -> Result<NextMilestoneOutcome> {
        let index = self.get_status()?;
        let backlog = self.get_backlog()?;
        let backlog_task_count = backlog
            .tasks
            .iter()
            .filter(|task| task.status != TaskflowStatus::Done)
            .count();

        if backlog_task_count > 0 {
            let pending_tasks: Vec<String> = backlog
                .tasks
                .iter()
                .filter(|t| t.status != TaskflowStatus::Done)
                .map(|t| t.task.clone())
                .collect();
            return Ok(NextMilestoneOutcome::Ready {
                milestone_id: "backlog".to_string(),
                name: "Global Backlog".to_string(),
                task_count: backlog_task_count,
                tasks: pending_tasks,
            });
        }

        // Build a status lookup from the index (cheap, no file I/O per entry).
        let status_map: std::collections::HashMap<String, String> = index
            .milestones
            .iter()
            .map(|(k, v)| (k.clone(), v.status.clone()))
            .collect();

        // Collect pending milestone IDs and load their depends_on.
        let mut pending: Vec<(u32, String, String)> = Vec::new(); // (numeric_id, key, name)
        let mut blocked_by: Vec<String> = Vec::new();

        let roadmap_dir = self.storage().designs_dir().join("roadmap");

        for (key, m_ref) in &index.milestones {
            if m_ref.status != "pending" {
                continue;
            }

            let m_path = roadmap_dir.join(&m_ref.path);
            let mut lock = LockedFile::open_shared(&m_path)?;
            let milestone: Milestone = lock.read_toml()?;

            let all_deps_done = milestone
                .depends_on
                .iter()
                .all(|dep| status_map.get(dep).map(|s| s == "done").unwrap_or(false));

            if all_deps_done {
                pending.push((milestone.id, key.clone(), m_ref.name.clone()));
            } else {
                for dep in &milestone.depends_on {
                    if status_map.get(dep).map(|s| s != "done").unwrap_or(true)
                        && !blocked_by.contains(dep)
                    {
                        blocked_by.push(dep.clone());
                    }
                }
            }
        }

        if pending.is_empty() {
            if blocked_by.is_empty() {
                return Ok(NextMilestoneOutcome::Exhausted);
            } else {
                blocked_by.sort();
                return Ok(NextMilestoneOutcome::Blocked {
                    waiting_on: blocked_by,
                });
            }
        }

        // Greedy: lowest numeric ID among ready milestones.
        pending.sort_by_key(|(id, _, _)| *id);
        let (_, key, name) = pending.remove(0);

        // Load task count from the milestone file.
        let m_ref = &index.milestones[&key];
        let m_path = roadmap_dir.join(&m_ref.path);
        let mut lock = LockedFile::open_shared(&m_path)?;
        let milestone: Milestone = lock.read_toml()?;
        let pending_tasks: Vec<String> = milestone
            .tasks
            .iter()
            .filter(|t| t.status != TaskflowStatus::Done)
            .map(|t| t.task.clone())
            .collect();
        let task_count = pending_tasks.len();

        Ok(NextMilestoneOutcome::Ready {
            milestone_id: key,
            name,
            task_count,
            tasks: pending_tasks,
        })
    }

    pub fn sync(&self) -> Result<SyncReport> {
        let mut report = SyncReport {
            updated_tasks: Vec::new(),
        };
        let links = self.links().list_all()?;
        let index = self.get_status()?;

        // 1. Sync Milestones
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
                milestone.status = Self::derive_milestone_status(&milestone.tasks)
                    .as_str()
                    .to_string();
                lock.write_toml_atomic(&milestone)?;
            }
        }

        // 2. Sync Backlog
        let backlog_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("backlog.toml");

        if backlog_path.exists() {
            let mut lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = lock.read_toml()?;
            let mut modified = false;

            for task in &mut backlog.tasks {
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
                lock.write_toml_atomic(&backlog)?;
            }
        }

        Ok(report)
    }

    pub fn set_task_status(&self, milestone_id: &str, task_id: &str, status: &str) -> Result<()> {
        let new_status = Self::resolve_taskflow_status(status)?;

        if milestone_id == "backlog" {
            let backlog_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("backlog.toml");
            let mut lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = lock.read_toml()?;
            let task = backlog
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .ok_or_else(|| aegis_core::error::AegisError::ConfigValidation {
                    field: "task_id".into(),
                    reason: format!("Task ID {} not found in backlog", task_id),
                })?;

            task.status = new_status;
            lock.write_toml_atomic(&backlog)?;
            return Ok(());
        }

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

        let mut milestone_lock = LockedFile::open_exclusive(&m_path)?;
        let mut milestone: Milestone = milestone_lock.read_toml()?;
        let task = milestone
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or_else(|| aegis_core::error::AegisError::ConfigValidation {
                field: "task_id".into(),
                reason: format!("Task ID {} not found in milestone {}", task_id, full_m_id),
            })?;

        task.status = new_status;
        milestone.status = Self::derive_milestone_status(&milestone.tasks)
            .as_str()
            .to_string();
        milestone_lock.write_toml_atomic(&milestone)?;

        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut refreshed_index: ProjectIndex = index_lock.read_toml()?;
        if let Some(m_ref_mut) = refreshed_index.milestones.get_mut(&full_m_id) {
            m_ref_mut.status = milestone.status.clone();
        }
        index_lock.write_toml_atomic(&refreshed_index)?;

        Ok(())
    }

    pub fn create_task(
        &self,
        milestone_id: &str,
        draft: TaskDraft,
    ) -> Result<crate::model::ProjectTask> {
        let target_id = Self::normalize_milestone_id(milestone_id);
        let status = draft.status.clone().unwrap_or(TaskflowStatus::Pending);

        if target_id == "backlog" {
            let backlog_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("backlog.toml");

            if let Some(parent) = backlog_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    aegis_core::error::AegisError::StorageIo {
                        path: parent.to_path_buf(),
                        source: e,
                    }
                })?;
            }

            let existed = backlog_path.exists();
            let mut lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = if existed {
                lock.read_toml()?
            } else {
                Backlog { tasks: Vec::new() }
            };

            let id = draft
                .id
                .filter(|id| !id.trim().is_empty())
                .unwrap_or_else(|| Self::next_backlog_task_id(&backlog.tasks));

            if self.task_id_conflicts(&id, None, std::slice::from_ref(&backlog_path))? {
                return Err(aegis_core::error::AegisError::ConfigValidation {
                    field: "task_id".into(),
                    reason: format!("Task ID {} already exists", id),
                });
            }

            let task = crate::model::ProjectTask {
                id,
                uid: Uuid::new_v4(),
                task: draft.task,
                task_type: draft.task_type,
                status,
                crate_name: draft.crate_name,
                notes: draft.notes,
                registry_task_id: None,
            };
            backlog.tasks.push(task.clone());
            lock.write_toml_atomic(&backlog)?;
            return Ok(task);
        }

        let index = self.get_status()?;
        let m_ref = index.milestones.get(&target_id).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone".into(),
                reason: format!("Milestone {} not found in index", target_id),
            }
        })?;
        let m_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&m_ref.path);
        let mut lock = LockedFile::open_exclusive(&m_path)?;
        let mut milestone: Milestone = lock.read_toml()?;

        let id = draft
            .id
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| Self::next_milestone_task_id(&milestone));

        if self.task_id_conflicts(&id, None, std::slice::from_ref(&m_path))? {
            return Err(aegis_core::error::AegisError::ConfigValidation {
                field: "task_id".into(),
                reason: format!("Task ID {} already exists", id),
            });
        }

        let task = crate::model::ProjectTask {
            id,
            uid: Uuid::new_v4(),
            task: draft.task,
            task_type: draft.task_type,
            status,
            crate_name: draft.crate_name,
            notes: draft.notes,
            registry_task_id: None,
        };
        milestone.tasks.push(task.clone());
        milestone.status = Self::derive_milestone_status(&milestone.tasks)
            .as_str()
            .to_string();
        lock.write_toml_atomic(&milestone)?;

        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut refreshed_index: ProjectIndex = index_lock.read_toml()?;
        if let Some(m_ref_mut) = refreshed_index.milestones.get_mut(&target_id) {
            m_ref_mut.status = milestone.status.clone();
        }
        index_lock.write_toml_atomic(&refreshed_index)?;

        Ok(task)
    }

    pub fn update_task(
        &self,
        source_milestone_id: &str,
        task_uid: Uuid,
        patch: TaskPatch,
    ) -> Result<crate::model::ProjectTask> {
        let source_id = Self::normalize_milestone_id(source_milestone_id);
        let target_id = patch
            .target_milestone_id
            .as_deref()
            .map(Self::normalize_milestone_id)
            .unwrap_or_else(|| source_id.clone());

        if source_id == "backlog" {
            let backlog_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("backlog.toml");
            let existed = backlog_path.exists();
            let mut backlog_lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = if existed {
                backlog_lock.read_toml()?
            } else {
                Backlog { tasks: Vec::new() }
            };
            let current_index = backlog
                .tasks
                .iter()
                .position(|task| task.uid == task_uid)
                .ok_or_else(|| aegis_core::error::AegisError::ConfigValidation {
                    field: "task_uid".into(),
                    reason: format!("Task {} not found in backlog", task_uid),
                })?;

            let mut updated = backlog.tasks[current_index].clone();
            Self::apply_task_patch(&mut updated, &patch);

            if self.task_id_conflicts(
                &updated.id,
                Some(task_uid),
                std::slice::from_ref(&backlog_path),
            )? {
                return Err(aegis_core::error::AegisError::ConfigValidation {
                    field: "task_id".into(),
                    reason: format!("Task ID {} already exists", updated.id),
                });
            }

            if target_id == "backlog" {
                backlog.tasks[current_index] = updated.clone();
                backlog_lock.write_toml_atomic(&backlog)?;
                return Ok(updated);
            }

            backlog.tasks.remove(current_index);
            backlog_lock.write_toml_atomic(&backlog)?;
            return self.insert_task_into_target(updated, &target_id);
        }

        let index = self.get_status()?;
        let m_ref = index.milestones.get(&source_id).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone".into(),
                reason: format!("Milestone {} not found in index", source_id),
            }
        })?;
        let m_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&m_ref.path);
        let mut milestone_lock = LockedFile::open_exclusive(&m_path)?;
        let mut milestone: Milestone = milestone_lock.read_toml()?;
        let current_index = milestone
            .tasks
            .iter()
            .position(|task| task.uid == task_uid)
            .ok_or_else(|| aegis_core::error::AegisError::ConfigValidation {
                field: "task_uid".into(),
                reason: format!("Task {} not found in milestone {}", task_uid, source_id),
            })?;

        let mut updated = milestone.tasks[current_index].clone();
        Self::apply_task_patch(&mut updated, &patch);

        if self.task_id_conflicts(&updated.id, Some(task_uid), std::slice::from_ref(&m_path))? {
            return Err(aegis_core::error::AegisError::ConfigValidation {
                field: "task_id".into(),
                reason: format!("Task ID {} already exists", updated.id),
            });
        }

        if target_id == source_id {
            milestone.tasks[current_index] = updated.clone();
            milestone.status = Self::derive_milestone_status(&milestone.tasks)
                .as_str()
                .to_string();
            milestone_lock.write_toml_atomic(&milestone)?;

            let index_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("index.toml");
            let mut index_lock = LockedFile::open_exclusive(&index_path)?;
            let mut refreshed_index: ProjectIndex = index_lock.read_toml()?;
            if let Some(m_ref_mut) = refreshed_index.milestones.get_mut(&source_id) {
                m_ref_mut.status = milestone.status.clone();
            }
            index_lock.write_toml_atomic(&refreshed_index)?;
            return Ok(updated);
        }

        milestone.tasks.remove(current_index);
        milestone.status = Self::derive_milestone_status(&milestone.tasks)
            .as_str()
            .to_string();
        milestone_lock.write_toml_atomic(&milestone)?;

        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut refreshed_index: ProjectIndex = index_lock.read_toml()?;
        if let Some(m_ref_mut) = refreshed_index.milestones.get_mut(&source_id) {
            m_ref_mut.status = milestone.status.clone();
        }
        index_lock.write_toml_atomic(&refreshed_index)?;

        self.insert_task_into_target(updated, &target_id)
    }

    fn insert_task_into_target(
        &self,
        task: crate::model::ProjectTask,
        target_milestone_id: &str,
    ) -> Result<crate::model::ProjectTask> {
        if target_milestone_id == "backlog" {
            let backlog_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("backlog.toml");
            if let Some(parent) = backlog_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    aegis_core::error::AegisError::StorageIo {
                        path: parent.to_path_buf(),
                        source: e,
                    }
                })?;
            }

            let existed = backlog_path.exists();
            let mut lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = if existed {
                lock.read_toml()?
            } else {
                Backlog { tasks: Vec::new() }
            };
            backlog.tasks.push(task.clone());
            lock.write_toml_atomic(&backlog)?;
            return Ok(task);
        }

        let index = self.get_status()?;
        let m_ref = index.milestones.get(target_milestone_id).ok_or_else(|| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone".into(),
                reason: format!("Milestone {} not found in index", target_milestone_id),
            }
        })?;
        let m_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join(&m_ref.path);
        let mut lock = LockedFile::open_exclusive(&m_path)?;
        let mut milestone: Milestone = lock.read_toml()?;
        milestone.tasks.push(task.clone());
        milestone.status = Self::derive_milestone_status(&milestone.tasks)
            .as_str()
            .to_string();
        lock.write_toml_atomic(&milestone)?;

        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");
        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut refreshed_index: ProjectIndex = index_lock.read_toml()?;
        if let Some(m_ref_mut) = refreshed_index.milestones.get_mut(target_milestone_id) {
            m_ref_mut.status = milestone.status.clone();
        }
        index_lock.write_toml_atomic(&refreshed_index)?;

        Ok(task)
    }

    pub fn create_milestone(&self, id: &str, name: &str, lld: Option<&str>) -> Result<()> {
        let index_path = self
            .storage()
            .designs_dir()
            .join("roadmap")
            .join("index.toml");

        let mut index_lock = LockedFile::open_exclusive(&index_path)?;
        let mut index: ProjectIndex = index_lock.read_toml()?;

        let milestone_id_num: u32 = id.strip_prefix('M').unwrap_or(id).parse().map_err(|_| {
            aegis_core::error::AegisError::ConfigValidation {
                field: "milestone_id".into(),
                reason: "Milestone ID must be a number (optionally prefixed with 'M')".into(),
            }
        })?;

        let id_clean = id.strip_prefix('M').unwrap_or(id);
        let filename = format!("M{}.toml", id_clean);
        let rel_path = format!("milestones/{}", filename);
        let full_path = self.storage().designs_dir().join("roadmap").join(&rel_path);

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
            depends_on: Vec::new(),
            tasks: Vec::new(),
        };

        // Write milestone file (exclusive because it's new)
        let mut m_lock = LockedFile::open_exclusive(&full_path)?;
        m_lock.write_toml_atomic(&milestone)?;

        // Update index
        index.milestones.insert(
            format!("M{}", id),
            crate::model::MilestoneRef {
                name: name.to_string(),
                path: rel_path,
                status: "pending".to_string(),
            },
        );

        index_lock.write_toml_atomic(&index)?;

        Ok(())
    }

    pub fn add_task(
        &self,
        milestone_id: &str,
        task_id: &str,
        task_desc: &str,
        task_type: crate::model::TaskType,
    ) -> Result<()> {
        if milestone_id == "backlog" {
            let backlog_path = self
                .storage()
                .designs_dir()
                .join("roadmap")
                .join("backlog.toml");

            // Create directory if it doesn't exist
            if let Some(parent) = backlog_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    aegis_core::error::AegisError::StorageIo {
                        path: parent.to_path_buf(),
                        source: e,
                    }
                })?;
            }

            let mut lock = LockedFile::open_exclusive(&backlog_path)?;
            let mut backlog: Backlog = if backlog_path.exists() {
                lock.read_toml()?
            } else {
                Backlog { tasks: Vec::new() }
            };

            if backlog.tasks.iter().any(|t| t.id == task_id) {
                return Err(aegis_core::error::AegisError::ConfigValidation {
                    field: "task_id".into(),
                    reason: format!("Task ID {} already exists in backlog", task_id),
                });
            }

            backlog.tasks.push(crate::model::ProjectTask {
                id: task_id.to_string(),
                uid: Uuid::new_v4(),
                task: task_desc.to_string(),
                task_type,
                status: TaskflowStatus::Pending,
                crate_name: None,
                notes: None,
                registry_task_id: None,
            });

            lock.write_toml_atomic(&backlog)?;
            return Ok(());
        }

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
                reason: format!(
                    "Task ID {} already exists in milestone {}",
                    task_id, full_m_id
                ),
            });
        }

        milestone.tasks.push(crate::model::ProjectTask {
            id: task_id.to_string(),
            uid: Uuid::new_v4(),
            task: task_desc.to_string(),
            task_type,
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
            self.tasks
                .lock()
                .unwrap()
                .insert(task.task_id, task.clone());
            Ok(())
        }
        fn get(&self, task_id: Uuid) -> aegis_core::Result<Option<aegis_core::Task>> {
            Ok(self.tasks.lock().unwrap().get(&task_id).cloned())
        }
        fn update_status(
            &self,
            _task_id: Uuid,
            _status: aegis_core::TaskStatus,
        ) -> aegis_core::Result<()> {
            Ok(())
        }
        fn assign(&self, _task_id: Uuid, _agent_id: Uuid) -> aegis_core::Result<()> {
            Ok(())
        }
        fn complete(
            &self,
            _task_id: Uuid,
            _receipt_path: Option<PathBuf>,
        ) -> aegis_core::Result<()> {
            Ok(())
        }
        fn list_pending(&self) -> aegis_core::Result<Vec<aegis_core::Task>> {
            Ok(vec![])
        }
        fn list_all(&self) -> aegis_core::Result<Vec<aegis_core::Task>> {
            Ok(self.tasks.lock().unwrap().values().cloned().collect())
        }
    }

    fn setup_engine() -> (TempDir, TaskflowEngine) {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(TestStorage {
            root: tmp.path().to_path_buf(),
        });
        let registry = Arc::new(MockTaskRegistry {
            tasks: std::sync::Mutex::new(HashMap::new()),
        });

        // Bootstrap minimal index
        let roadmap_dir = storage.designs_dir().join("roadmap");
        std::fs::create_dir_all(&roadmap_dir).unwrap();
        std::fs::create_dir_all(roadmap_dir.join("milestones")).unwrap();
        std::fs::create_dir_all(storage.state_dir()).unwrap();

        // Initialize blank taskflow links
        std::fs::write(storage.taskflow_path(), "{}").unwrap();

        let index = ProjectIndex {
            project: ProjectMeta {
                name: "Test".to_string(),
                current_milestone: 1,
                backlog: None,
            },
            milestones: HashMap::new(),
        };
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        (tmp, TaskflowEngine::new(storage, registry))
    }

    #[test]
    fn test_index_contains_backlog() {
        let (tmp, engine) = setup_engine();

        // Manual override of index with backlog
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let index = ProjectIndex {
            project: ProjectMeta {
                name: "Test".to_string(),
                current_milestone: 1,
                backlog: Some("backlog.toml".to_string()),
            },
            milestones: HashMap::new(),
        };
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let status = engine.get_status().unwrap();
        assert_eq!(status.project.backlog, Some("backlog.toml".to_string()));
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
        engine
            .add_task("M10", "10.1", "First task", crate::model::TaskType::Feature)
            .unwrap();

        let m = engine.get_milestone("M10").unwrap();
        assert_eq!(m.tasks.len(), 1);
        assert_eq!(m.tasks[0].id, "10.1");
        assert_eq!(m.tasks[0].task, "First task");
    }

    #[test]
    fn test_create_task_generates_backlog_id() {
        let (_tmp, engine) = setup_engine();
        let task = engine
            .create_task(
                "backlog",
                crate::model::TaskDraft {
                    id: None,
                    task: "Log bug".to_string(),
                    task_type: crate::model::TaskType::Bug,
                    status: None,
                    crate_name: None,
                    notes: None,
                },
            )
            .unwrap();

        assert_eq!(task.id, "B1");
        assert_eq!(task.task_type, crate::model::TaskType::Bug);

        let backlog = engine.get_backlog().unwrap();
        assert_eq!(backlog.tasks.len(), 1);
        assert_eq!(backlog.tasks[0].uid, task.uid);
    }

    #[test]
    fn test_update_task_can_edit_and_move_between_locations() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("34", "Web", None).unwrap();
        let task = engine
            .create_task(
                "M34",
                crate::model::TaskDraft {
                    id: Some("34.1".to_string()),
                    task: "Original".to_string(),
                    task_type: crate::model::TaskType::Feature,
                    status: None,
                    crate_name: None,
                    notes: None,
                },
            )
            .unwrap();

        let updated = engine
            .update_task(
                "M34",
                task.uid,
                crate::model::TaskPatch {
                    id: Some("B9".to_string()),
                    task: Some("Edited".to_string()),
                    task_type: Some(crate::model::TaskType::Bug),
                    status: Some(TaskflowStatus::InProgress),
                    crate_name: Some(Some("crate-a".to_string())),
                    notes: Some(Some("note".to_string())),
                    target_milestone_id: Some("backlog".to_string()),
                },
            )
            .unwrap();

        assert_eq!(updated.id, "B9");
        assert_eq!(updated.task, "Edited");
        assert_eq!(updated.task_type, crate::model::TaskType::Bug);
        assert_eq!(updated.status, TaskflowStatus::InProgress);
        assert_eq!(updated.crate_name.as_deref(), Some("crate-a"));
        assert_eq!(updated.notes.as_deref(), Some("note"));

        let backlog = engine.get_backlog().unwrap();
        assert_eq!(backlog.tasks.len(), 1);
        assert_eq!(backlog.tasks[0].uid, task.uid);
        assert_eq!(backlog.tasks[0].id, "B9");
        assert_eq!(backlog.tasks[0].task, "Edited");

        let milestone = engine.get_milestone("M34").unwrap();
        assert!(milestone.tasks.is_empty());
    }

    #[test]
    fn test_sync_updates_status() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("1", "M1", None).unwrap();
        engine
            .add_task("M1", "1.1", "Task 1", crate::model::TaskType::Feature)
            .unwrap();

        let task_uuid = Uuid::new_v4();
        engine
            .registry()
            .insert(&aegis_core::Task {
                task_id: task_uuid,
                description: "Task 1".to_string(),
                status: aegis_core::TaskStatus::Complete,
                assigned_agent_id: None,
                created_by: TaskCreator::System,
                created_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                receipt_path: None,
            })
            .unwrap();

        engine.links().assign("1.1".to_string(), task_uuid).unwrap();

        let report = engine.sync().unwrap();
        assert_eq!(report.updated_tasks.len(), 1);
        assert_eq!(report.updated_tasks[0], "1.1");

        let m = engine.get_milestone("M1").unwrap();
        assert_eq!(m.tasks[0].status, crate::model::TaskflowStatus::Done);
        assert_eq!(m.tasks[0].registry_task_id, Some(task_uuid));
    }

    #[test]
    fn test_set_task_status_updates_milestone_and_index() {
        let (_tmp, engine) = setup_engine();
        engine.create_milestone("15", "Web", None).unwrap();
        engine
            .add_task(
                "M15",
                "15.1",
                "Spawn agent",
                crate::model::TaskType::Feature,
            )
            .unwrap();

        engine.set_task_status("15", "15.1", "done").unwrap();

        let m = engine.get_milestone("M15").unwrap();
        assert_eq!(m.tasks[0].status, TaskflowStatus::Done);
        assert_eq!(m.status, "done");

        let index = engine.get_status().unwrap();
        assert_eq!(index.milestones.get("M15").unwrap().status, "done");
    }

    fn make_milestone_file(dir: &std::path::Path, id: u32, status: &str, depends_on: &[&str]) {
        let m = crate::model::Milestone {
            id,
            name: format!("Milestone {id}"),
            status: status.to_string(),
            lld: None,
            depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
            tasks: vec![crate::model::ProjectTask {
                id: format!("{id}.1"),
                uid: Uuid::new_v4(),
                task: "a task".to_string(),
                task_type: crate::model::TaskType::Feature,
                status: if status == "done" {
                    TaskflowStatus::Done
                } else {
                    TaskflowStatus::Pending
                },
                crate_name: None,
                notes: None,
                registry_task_id: None,
            }],
        };
        let path = dir.join(format!("M{id}.toml"));
        std::fs::write(&path, toml::to_string(&m).unwrap()).unwrap();
    }

    fn add_milestone_ref(index: &mut ProjectIndex, id: u32, status: &str) {
        index.milestones.insert(
            format!("M{id}"),
            crate::model::MilestoneRef {
                name: format!("Milestone {id}"),
                path: format!("milestones/M{id}.toml"),
                status: status.to_string(),
            },
        );
    }

    fn write_backlog(roadmap_dir: &std::path::Path, statuses: &[TaskflowStatus]) {
        let backlog = Backlog {
            tasks: statuses
                .iter()
                .enumerate()
                .map(|(index, status)| crate::model::ProjectTask {
                    id: format!("B{}", index + 1),
                    uid: Uuid::new_v4(),
                    task: format!("backlog task {}", index + 1),
                    task_type: crate::model::TaskType::Feature,
                    status: status.clone(),
                    crate_name: None,
                    notes: None,
                    registry_task_id: None,
                })
                .collect(),
        };
        std::fs::write(
            roadmap_dir.join("backlog.toml"),
            toml::to_string(&backlog).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn next_returns_lowest_ready_milestone() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        make_milestone_file(&m_dir, 5, "pending", &[]);
        make_milestone_file(&m_dir, 3, "pending", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 5, "pending");
        add_milestone_ref(&mut index, 3, "pending");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        assert!(
            matches!(outcome, NextMilestoneOutcome::Ready { milestone_id, .. } if milestone_id == "M3")
        );
        drop(tmp);
    }

    #[test]
    fn next_prioritizes_global_backlog_before_milestones() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        write_backlog(
            &roadmap_dir,
            &[TaskflowStatus::Pending, TaskflowStatus::Done],
        );
        make_milestone_file(&m_dir, 1, "pending", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 1, "pending");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        assert!(
            matches!(outcome, NextMilestoneOutcome::Ready { milestone_id, name, task_count, .. }
                if milestone_id == "backlog" && name == "Global Backlog" && task_count == 1)
        );
        drop(tmp);
    }

    #[test]
    fn next_uses_milestones_when_global_backlog_is_done() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        write_backlog(&roadmap_dir, &[TaskflowStatus::Done]);
        make_milestone_file(&m_dir, 2, "pending", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 2, "pending");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        assert!(
            matches!(outcome, NextMilestoneOutcome::Ready { milestone_id, .. } if milestone_id == "M2")
        );
        drop(tmp);
    }

    #[test]
    fn next_skips_milestone_with_unmet_dep() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        // M10 is pending with no deps — should be returned.
        // M11 depends on M12 which is pending — should be skipped.
        make_milestone_file(&m_dir, 10, "pending", &[]);
        make_milestone_file(&m_dir, 11, "pending", &["M12"]);
        make_milestone_file(&m_dir, 12, "pending", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 10, "pending");
        add_milestone_ref(&mut index, 11, "pending");
        add_milestone_ref(&mut index, 12, "pending");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        // M10 has no deps, M12 has no deps — both ready; M10 is lower.
        assert!(
            matches!(outcome, NextMilestoneOutcome::Ready { milestone_id, .. } if milestone_id == "M10")
        );
        drop(tmp);
    }

    #[test]
    fn next_returns_exhausted_when_all_done() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        make_milestone_file(&m_dir, 1, "done", &[]);
        make_milestone_file(&m_dir, 2, "done", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 1, "done");
        add_milestone_ref(&mut index, 2, "done");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        assert!(matches!(outcome, NextMilestoneOutcome::Exhausted));
        drop(tmp);
    }

    #[test]
    fn next_returns_blocked_when_only_unmet_deps_remain() {
        let (tmp, engine) = setup_engine();
        let roadmap_dir = engine.storage().designs_dir().join("roadmap");
        let m_dir = roadmap_dir.join("milestones");

        // M20 is pending but depends on M19 which is also pending.
        // No other ready milestones exist.
        make_milestone_file(&m_dir, 20, "pending", &["M19"]);
        make_milestone_file(&m_dir, 19, "pending", &[]);

        let mut index: ProjectIndex =
            toml::from_str(&std::fs::read_to_string(roadmap_dir.join("index.toml")).unwrap())
                .unwrap();
        add_milestone_ref(&mut index, 20, "pending");
        // M19 is marked done in the index but pending on disk — use done in index to block M20,
        // keep M19 itself not returned.
        // Actually: let's make M19 done in the index (already merged) but M20 still pending.
        // Then the only pending is M20, which depends on a done M19.
        // That would make M20 ready. Instead: mark M19 as pending in the index too, and
        // remove M19's TOML entry — so M20 depends on something that doesn't exist (unknown).
        // depends_on check: status_map.get("M19") returns None → unknown → not done → blocked.
        index.milestones.remove("M19");
        add_milestone_ref(&mut index, 20, "pending");
        std::fs::write(
            roadmap_dir.join("index.toml"),
            toml::to_string(&index).unwrap(),
        )
        .unwrap();

        let outcome = engine.next_milestone().unwrap();
        assert!(
            matches!(&outcome, NextMilestoneOutcome::Blocked { waiting_on } if waiting_on.contains(&"M19".to_string()))
        );
        drop(tmp);
    }
}
