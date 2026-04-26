use aegis_core::{AegisError, Channel, ChannelKind, Message, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

pub struct MailboxChannel {
    name: String,
    inbox_path: PathBuf,
}

impl MailboxChannel {
    pub fn new(name: String, inbox_path: PathBuf) -> Self {
        Self { name, inbox_path }
    }

    pub fn inbox_path(&self) -> &Path {
        &self.inbox_path
    }

    pub fn message_path(&self, message: &Message) -> PathBuf {
        self.inbox_path.join(format!(
            "{}_{}.json",
            message.created_at.timestamp(),
            message.message_id
        ))
    }

    pub fn list_messages(&self) -> Result<Vec<Message>> {
        if !self.inbox_path.exists() {
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        for entry in fs::read_dir(&self.inbox_path).map_err(|e| AegisError::StorageIo {
            path: self.inbox_path.clone(),
            source: e,
        })? {
            let entry = entry.map_err(|e| AegisError::StorageIo {
                path: self.inbox_path.clone(),
                source: e,
            })?;

            if !entry.file_type().map_err(|e| AegisError::StorageIo {
                path: entry.path(),
                source: e,
            })?
            .is_file()
            {
                continue;
            }

            if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let content = fs::read_to_string(entry.path()).map_err(|e| AegisError::StorageIo {
                path: entry.path(),
                source: e,
            })?;
            let message: Message =
                serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
                    path: entry.path(),
                    source: e,
                })?;
            messages.push(message);
        }

        messages.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.message_id.cmp(&b.message_id))
        });

        Ok(messages)
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

        let path = self.message_path(message);

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

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{MessageSource, MessageType};
    use chrono::TimeZone;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn message(created_at: chrono::DateTime<Utc>, message_id: Uuid, payload: &str) -> Message {
        Message {
            message_id,
            from: MessageSource::System,
            to_agent_id: Uuid::new_v4(),
            kind: MessageType::Notification,
            priority: 0,
            payload: serde_json::Value::String(payload.to_string()),
            created_at,
        }
    }

    #[test]
    fn list_messages_sorts_by_created_at_then_id() {
        let dir = tempdir().unwrap();
        let mailbox = MailboxChannel::new("mailbox".into(), dir.path().to_path_buf());

        let later = message(
            Utc.timestamp_opt(20, 0).single().unwrap(),
            Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap(),
            "later",
        );
        let earlier = message(
            Utc.timestamp_opt(10, 0).single().unwrap(),
            Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            "earlier",
        );

        std::fs::create_dir_all(mailbox.inbox_path()).unwrap();
        std::fs::write(mailbox.message_path(&later), serde_json::to_string(&later).unwrap())
            .unwrap();
        std::fs::write(mailbox.message_path(&earlier), serde_json::to_string(&earlier).unwrap())
            .unwrap();

        let messages = mailbox.list_messages().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].payload, serde_json::Value::String("earlier".into()));
        assert_eq!(messages[1].payload, serde_json::Value::String("later".into()));
    }

    #[test]
    fn list_messages_returns_empty_for_missing_inbox() {
        let dir = tempdir().unwrap();
        let mailbox = MailboxChannel::new("mailbox".into(), dir.path().join("missing"));

        let messages = mailbox.list_messages().unwrap();
        assert!(messages.is_empty());
    }
}
