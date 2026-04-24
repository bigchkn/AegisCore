use aegis_core::error::{AegisError, Result};
use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

pub mod agents;
pub mod channels;
pub mod tasks;

pub use agents::*;
pub use channels::*;
pub use tasks::*;

pub struct LockedFile {
    file: File,
    path: PathBuf,
}

impl LockedFile {
    pub fn open_exclusive(path: &Path) -> Result<Self> {
        Self::open(path, true)
    }

    pub fn open_shared(path: &Path) -> Result<Self> {
        Self::open(path, false)
    }

    fn open(path: &Path, exclusive: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(|e| AegisError::StorageIo {
                path: path.to_path_buf(),
                source: e,
            })?;

        let start = Instant::now();
        let timeout = Duration::from_secs(5);

        loop {
            let res = if exclusive {
                file.try_lock_exclusive().map_err(|e| e.to_string())
            } else {
                file.try_lock_shared().map_err(|e| e.to_string())
            };

            match res {
                Ok(_) => break,
                Err(_) if start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(AegisError::RegistryLock {
                        source: std::io::Error::new(std::io::ErrorKind::Other, e),
                    })
                }
            }
        }

        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    pub fn read_json<T: DeserializeOwned>(&mut self) -> Result<T> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| AegisError::StorageIo {
                path: self.path.clone(),
                source: e,
            })?;

        let mut content = String::new();
        self.file
            .read_to_string(&mut content)
            .map_err(|e| AegisError::StorageIo {
                path: self.path.clone(),
                source: e,
            })?;

        if content.trim().is_empty() {
            // Return empty/default if the file was just created
            return serde_json::from_str("{}").map_err(|e| AegisError::RegistryCorrupted {
                path: self.path.clone(),
                source: e,
            });
        }

        serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
            path: self.path.clone(),
            source: e,
        })
    }

    pub fn write_json_atomic<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = NamedTempFile::new_in(parent).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;

        let json =
            serde_json::to_string_pretty(value).map_err(|e| AegisError::RegistryCorrupted {
                path: self.path.clone(),
                source: e,
            })?;

        tmp.write_all(json.as_bytes())
            .map_err(|e| AegisError::StorageIo {
                path: self.path.clone(),
                source: e,
            })?;

        tmp.persist(&self.path).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e.error,
        })?;

        // Re-acquire lock on the new file
        self.file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| AegisError::StorageIo {
                path: self.path.clone(),
                source: e,
            })?;
        self.file
            .lock_exclusive()
            .map_err(|e| AegisError::RegistryLock { source: e })?;

        Ok(())
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
