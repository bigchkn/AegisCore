use aegis_core::{AegisError, ChannelKind, ChannelRecord, ChannelRegistry, Result};
use chrono::Utc;
use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub struct FileChannelRegistry {
    path: PathBuf,
}

impl FileChannelRegistry {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn lock(&self, exclusive: bool) -> Result<LockedFile> {
        let lock_path = self.path.with_extension("lock");
        LockedFile::open(&self.path, &lock_path, exclusive)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ChannelStore {
    version: u32,
    channels: Vec<ChannelRecord>,
}

impl ChannelRegistry for FileChannelRegistry {
    fn register(&self, name: &str, kind: ChannelKind) -> Result<()> {
        let mut file = self.lock(true)?;
        let mut store: ChannelStore = file.read_json()?;

        if store.channels.iter().any(|c| c.name == name) {
            return Ok(()); // Already registered
        }

        store.channels.push(ChannelRecord {
            name: name.to_string(),
            kind,
            active: true,
            registered_at: Utc::now(),
            config: serde_json::Value::Null,
        });

        file.write_json_atomic(&store)
    }

    fn deregister(&self, name: &str) -> Result<()> {
        let mut file = self.lock(true)?;
        let mut store: ChannelStore = file.read_json()?;

        store.channels.retain(|c| c.name != name);

        file.write_json_atomic(&store)
    }

    fn get(&self, name: &str) -> Result<Option<ChannelRecord>> {
        let mut file = self.lock(false)?;
        let store: ChannelStore = file.read_json()?;

        Ok(store.channels.into_iter().find(|c| c.name == name))
    }

    fn list(&self) -> Result<Vec<ChannelRecord>> {
        let mut file = self.lock(false)?;
        let store: ChannelStore = file.read_json()?;

        Ok(store.channels)
    }
}

/// Internal helper that holds a lock on a sidecar .lock file
/// while providing access to the registry data file.
struct LockedFile {
    lock_file: File,
    data_path: PathBuf,
}

impl LockedFile {
    fn open(data_path: &Path, lock_path: &Path, exclusive: bool) -> Result<Self> {
        if let Some(parent) = data_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| AegisError::StorageIo {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(lock_path)
            .map_err(|e| AegisError::StorageIo {
                path: lock_path.to_path_buf(),
                source: e,
            })?;

        if exclusive {
            lock_file.lock_exclusive()
        } else {
            lock_file.lock_shared()
        }
        .map_err(|e| AegisError::RegistryLock { source: e })?;

        Ok(Self {
            lock_file,
            data_path: data_path.to_path_buf(),
        })
    }

    fn read_json<T: DeserializeOwned + Default>(&mut self) -> Result<T> {
        if !self.data_path.exists() {
            return Ok(T::default());
        }

        let mut file = File::open(&self.data_path).map_err(|e| AegisError::StorageIo {
            path: self.data_path.clone(),
            source: e,
        })?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| AegisError::StorageIo {
                path: self.data_path.clone(),
                source: e,
            })?;

        if content.trim().is_empty() {
            return Ok(T::default());
        }

        serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
            path: self.data_path.clone(),
            source: e,
        })
    }

    fn write_json_atomic<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let parent = self.data_path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = NamedTempFile::new_in(parent).map_err(|e| AegisError::StorageIo {
            path: self.data_path.clone(),
            source: e,
        })?;

        let json =
            serde_json::to_string_pretty(value).map_err(|e| AegisError::RegistryCorrupted {
                path: self.data_path.clone(),
                source: e,
            })?;

        tmp.write_all(json.as_bytes())
            .map_err(|e| AegisError::StorageIo {
                path: self.data_path.clone(),
                source: e,
            })?;

        tmp.persist(&self.data_path).map_err(|e| AegisError::StorageIo {
            path: self.data_path.clone(),
            source: e.error,
        })?;

        Ok(())
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = self.lock_file.unlock();
    }
}
