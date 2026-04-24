use aegis_core::{AegisError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ProjectRecord {
    pub id: Uuid,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub root_path: PathBuf,
    pub auto_start: bool,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectStore {
    projects: Vec<ProjectRecord>,
}

pub struct ProjectRegistry {
    path: PathBuf,
}

impl ProjectRegistry {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let path = PathBuf::from(home).join(".aegis/state/projects.json");
        Self { path }
    }

    pub fn load(&self) -> Result<Vec<ProjectRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.path).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;

        let store: ProjectStore =
            serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
                path: self.path.clone(),
                source: e,
            })?;

        Ok(store.projects)
    }

    pub fn save(&self, projects: Vec<ProjectRecord>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| AegisError::StorageIo {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let store = ProjectStore { projects };
        let content =
            serde_json::to_string_pretty(&store).map_err(|e| AegisError::RegistryCorrupted {
                path: self.path.clone(),
                source: e,
            })?;

        // Atomic write via tempfile
        let mut tmp = tempfile::NamedTempFile::new().map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;

        use std::io::Write;
        tmp.write_all(content.as_bytes())
            .map_err(|e| AegisError::StorageIo {
                path: self.path.clone(),
                source: e,
            })?;

        tmp.persist(&self.path).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e.error,
        })?;

        Ok(())
    }

    pub fn register(&self, root_path: PathBuf) -> Result<ProjectRecord> {
        let mut projects = self.load()?;

        // Canonicalize to avoid duplicate entries with different path formats
        let abs_path = root_path
            .canonicalize()
            .map_err(|e| AegisError::StorageIo {
                path: root_path.clone(),
                source: e,
            })?;

        if let Some(existing) = projects.iter_mut().find(|p| p.root_path == abs_path) {
            existing.last_seen = Utc::now();
            let record = existing.clone();
            self.save(projects)?;
            return Ok(record);
        }

        let record = ProjectRecord {
            id: Uuid::new_v4(),
            root_path: abs_path,
            auto_start: true,
            last_seen: Utc::now(),
        };

        projects.push(record.clone());
        self.save(projects)?;
        Ok(record)
    }

    pub fn unregister(&self, id: Uuid) -> Result<()> {
        let mut projects = self.load()?;
        projects.retain(|p| p.id != id);
        self.save(projects)
    }

    pub fn find_by_path(&self, path: &PathBuf) -> Result<Option<ProjectRecord>> {
        let abs = path.canonicalize().unwrap_or_else(|_| path.clone());
        Ok(self.load()?.into_iter().find(|p| p.root_path == abs))
    }
}
