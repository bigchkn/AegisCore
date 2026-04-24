use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::{Serialize, Deserialize};
use aegis_core::{ChannelKind, ChannelRecord, ChannelRegistry, Result, AegisError};
use chrono::Utc;
use tempfile::NamedTempFile;

pub struct FileChannelRegistry {
    path: PathBuf,
}

impl FileChannelRegistry {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn lock(&self, exclusive: bool) -> Result<LockedFile> {
        LockedFile::open(&self.path, exclusive)
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

// Internal helper mirroring the one in aegis-controller
struct LockedFile {
    file: File,
    path: PathBuf,
}

impl LockedFile {
    fn open(path: &Path, exclusive: bool) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| AegisError::StorageIo {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

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
                Err(e) => return Err(AegisError::RegistryLock { 
                    source: std::io::Error::new(std::io::ErrorKind::Other, e) 
                }),
            }
        }

        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    fn read_json<T: DeserializeOwned + Default>(&mut self) -> Result<T> {
        self.file.seek(SeekFrom::Start(0)).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;
        
        let mut content = String::new();
        self.file.read_to_string(&mut content).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;

        if content.trim().is_empty() {
            return Ok(T::default());
        }

        serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
            path: self.path.clone(),
            source: e,
        })
    }

    fn write_json_atomic<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = NamedTempFile::new_in(parent).map_err(|e| AegisError::StorageIo {
            path: self.path.clone(),
            source: e,
        })?;

        let json = serde_json::to_string_pretty(value).map_err(|e| AegisError::RegistryCorrupted {
            path: self.path.clone(),
            source: e,
        })?;

        tmp.write_all(json.as_bytes()).map_err(|e| AegisError::StorageIo {
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
        self.file.lock_exclusive().map_err(|e| AegisError::RegistryLock { source: e })?;

        Ok(())
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
