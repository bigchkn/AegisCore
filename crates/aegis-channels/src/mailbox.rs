use aegis_core::{AegisError, Channel, ChannelKind, Message, Result};
use async_trait::async_trait;
use std::fs;
use std::path::PathBuf;

pub struct MailboxChannel {
    name: String,
    inbox_path: PathBuf,
}

impl MailboxChannel {
    pub fn new(name: String, inbox_path: PathBuf) -> Self {
        Self { name, inbox_path }
    }
}

#[async_trait]
impl Channel for MailboxChannel {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Mailbox
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_active(&self) -> bool {
        self.inbox_path.exists()
    }

    async fn send(&self, message: &Message) -> Result<()> {
        if !self.inbox_path.exists() {
            fs::create_dir_all(&self.inbox_path).map_err(|e| AegisError::StorageIo {
                path: self.inbox_path.clone(),
                source: e,
            })?;
        }

        let filename = format!(
            "{}_{}.json",
            message.created_at.timestamp(),
            message.message_id
        );
        let path = self.inbox_path.join(filename);

        // Atomic write via tempfile
        let inbox_path = self.inbox_path.clone();
        let message = message.clone();

        tokio::task::spawn_blocking(move || {
            let mut tmp = tempfile::NamedTempFile::new_in(&inbox_path).map_err(|e| {
                AegisError::StorageIo {
                    path: inbox_path.clone(),
                    source: e,
                }
            })?;

            serde_json::to_writer_pretty(&mut tmp, &message).map_err(|e| {
                AegisError::RegistryCorrupted {
                    path: inbox_path.clone(),
                    source: e,
                }
            })?;

            tmp.persist(&path).map_err(|e| AegisError::StorageIo {
                path,
                source: e.error,
            })?;

            Ok(())
        })
        .await
        .map_err(|e| AegisError::Unexpected(Box::new(e)))?
    }
}
