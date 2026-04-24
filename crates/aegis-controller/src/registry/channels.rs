use crate::registry::{FileRegistry, LockedFile};
use aegis_core::channel::{ChannelKind, ChannelRecord, ChannelRegistry};
use aegis_core::error::{AegisError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ChannelStore {
    pub version: u32,
    pub channels: Vec<ChannelRecord>,
}

impl ChannelRegistry for FileRegistry {
    fn register(&self, name: &str, kind: ChannelKind) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.channels_state_path())?;
        let mut store: ChannelStore = file.read_json()?;

        if store.channels.iter().any(|c| c.name == name) {
            // Already registered
            return Ok(());
        }

        let record = ChannelRecord {
            name: name.to_string(),
            kind,
            active: true,
            registered_at: Utc::now(),
            config: serde_json::Value::Null,
        };

        store.channels.push(record);
        file.write_json_atomic(&store)
    }

    fn deregister(&self, name: &str) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.channels_state_path())?;
        let mut store: ChannelStore = file.read_json()?;

        if let Some(pos) = store.channels.iter().position(|c| c.name == name) {
            store.channels.remove(pos);
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::ChannelNotFound {
                name: name.to_string(),
            })
        }
    }

    fn get(&self, name: &str) -> Result<Option<ChannelRecord>> {
        let mut file = LockedFile::open_shared(&self.storage.channels_state_path())?;
        let store: ChannelStore = file.read_json()?;
        Ok(store.channels.iter().find(|c| c.name == name).cloned())
    }

    fn list(&self) -> Result<Vec<ChannelRecord>> {
        let mut file = LockedFile::open_shared(&self.storage.channels_state_path())?;
        let store: ChannelStore = file.read_json()?;
        Ok(store.channels.clone())
    }
}
