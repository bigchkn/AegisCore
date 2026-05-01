use crate::error::{AegisError, Result};
use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub struct LockedFile {
    lock_file: File,
    data_path: PathBuf,
}

impl LockedFile {
    pub fn open_exclusive(path: &Path) -> Result<Self> {
        Self::open_with_lock(path, true)
    }

    pub fn open_shared(path: &Path) -> Result<Self> {
        Self::open_with_lock(path, false)
    }

    fn open_with_lock(data_path: &Path, exclusive: bool) -> Result<Self> {
        let lock_path = data_path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
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
            .open(&lock_path)
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

    pub fn read_json<T: DeserializeOwned>(&mut self) -> Result<T> {
        if !self.data_path.exists() {
            return serde_json::from_str("{}").map_err(|e| AegisError::RegistryCorrupted {
                path: self.data_path.clone(),
                source: e,
            });
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
            return serde_json::from_str("{}").map_err(|e| AegisError::RegistryCorrupted {
                path: self.data_path.clone(),
                source: e,
            });
        }

        serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
            path: self.data_path.clone(),
            source: e,
        })
    }

    pub fn read_toml<T: DeserializeOwned>(&mut self) -> Result<T> {
        if !self.data_path.exists() {
            return toml::from_str("").map_err(|e| AegisError::Config {
                field: self.data_path.display().to_string(),
                reason: format!("Empty TOML: {}", e),
            });
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
            return toml::from_str("").map_err(|e| AegisError::Config {
                field: self.data_path.display().to_string(),
                reason: format!("Empty TOML: {}", e),
            });
        }

        toml::from_str(&content).map_err(|e| AegisError::Config {
            field: self.data_path.display().to_string(),
            reason: format!("TOML Parse Error: {}", e),
        })
    }

    pub fn write_json_atomic<T: Serialize>(&mut self, value: &T) -> Result<()> {
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

    pub fn write_toml_atomic<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let parent = self.data_path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = NamedTempFile::new_in(parent).map_err(|e| AegisError::StorageIo {
            path: self.data_path.clone(),
            source: e,
        })?;

        let content =
            toml::to_string_pretty(value).map_err(|e| AegisError::ConfigSerializationError {
                path: self.data_path.clone(),
                source: e,
            })?;

        tmp.write_all(content.as_bytes())
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
