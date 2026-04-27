use std::path::PathBuf;
use std::sync::Arc;

use aegis_channels::MailboxChannel;
use aegis_core::{
    AegisError, Agent, AgentRegistry, AgentStatus, Channel, Message, MessageSource, MessageType,
    Result, StorageBackend,
};
use aegis_tmux::{TmuxClient, TmuxTarget};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::registry::FileRegistry;
use crate::storage::ProjectStorage;
use crate::transcript::append_tmux_send;

#[derive(Clone)]
pub struct MessageRouter {
    registry: Arc<FileRegistry>,
    storage: Arc<ProjectStorage>,
    tmux: Option<Arc<TmuxClient>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDeliveryReceipt {
    pub message_id: Uuid,
    pub to_agent_id: Uuid,
    pub inbox_path: PathBuf,
    pub nudged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageInbox {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub agent_status: AgentStatus,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageInboxSummary {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub agent_status: AgentStatus,
    pub queued_messages: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_message_at: Option<DateTime<Utc>>,
}

impl MessageRouter {
    pub fn new(
        registry: Arc<FileRegistry>,
        storage: Arc<ProjectStorage>,
        tmux: Option<Arc<TmuxClient>>,
    ) -> Self {
        Self {
            registry,
            storage,
            tmux,
        }
    }

    pub async fn send(
        &self,
        from_agent_id: Option<Uuid>,
        to_agent_raw: &str,
        kind: MessageType,
        payload: serde_json::Value,
    ) -> Result<MessageDeliveryReceipt> {
        let to_agent_id = self.resolve_agent_id(to_agent_raw)?;
        let agent = self.agent_by_id(to_agent_id)?;

        if let Some(from_agent_id) = from_agent_id {
            let sender_exists =
                AgentRegistry::get(self.registry.as_ref(), from_agent_id)?.is_some();
            if !sender_exists {
                return Err(AegisError::AgentNotFound {
                    agent_id: from_agent_id,
                });
            }
        }

        let message = Message::new(
            from_agent_id
                .map(MessageSource::Agent)
                .unwrap_or(MessageSource::System),
            to_agent_id,
            kind,
            payload,
        );

        let mailbox = MailboxChannel::new(
            format!("mailbox-{to_agent_id}"),
            self.storage.agent_inbox_path(to_agent_id),
        );
        mailbox.send(&message).await?;

        let (nudged, warning) = match self.nudge(&agent).await {
            Ok(nudged) => (nudged, None),
            Err(err) => (false, Some(err.to_string())),
        };

        Ok(MessageDeliveryReceipt {
            message_id: message.message_id,
            to_agent_id,
            inbox_path: mailbox.inbox_path().to_path_buf(),
            nudged,
            warning,
        })
    }

    pub fn inbox(&self, agent_raw: &str) -> Result<MessageInbox> {
        let agent_id = self.resolve_agent_id(agent_raw)?;
        let agent = self.agent_by_id(agent_id)?;
        let mailbox = MailboxChannel::new(
            format!("mailbox-{agent_id}"),
            self.storage.agent_inbox_path(agent_id),
        );

        Ok(MessageInbox {
            agent_id,
            agent_name: agent.name,
            agent_status: agent.status,
            messages: mailbox.list_messages()?,
        })
    }

    pub fn list(&self) -> Result<Vec<MessageInboxSummary>> {
        let mut summaries = Vec::new();

        for agent in AgentRegistry::list_all(self.registry.as_ref())? {
            let mailbox = MailboxChannel::new(
                format!("mailbox-{}", agent.agent_id),
                self.storage.agent_inbox_path(agent.agent_id),
            );
            let messages = mailbox.list_messages()?;
            let newest_message_at = messages.iter().map(|m| m.created_at).max();

            summaries.push(MessageInboxSummary {
                agent_id: agent.agent_id,
                agent_name: agent.name,
                agent_status: agent.status,
                queued_messages: messages.len(),
                newest_message_at,
            });
        }

        summaries.sort_by(|a, b| {
            b.queued_messages
                .cmp(&a.queued_messages)
                .then_with(|| a.agent_name.cmp(&b.agent_name))
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });

        Ok(summaries)
    }

    fn agent_by_id(&self, agent_id: Uuid) -> Result<Agent> {
        AgentRegistry::get(self.registry.as_ref(), agent_id)?
            .ok_or(AegisError::AgentNotFound { agent_id })
    }

    fn resolve_agent_id(&self, raw: &str) -> Result<Uuid> {
        let agents = AgentRegistry::list_all(self.registry.as_ref())?;

        if let Ok(uuid) = Uuid::parse_str(raw) {
            if agents.iter().any(|agent| agent.agent_id == uuid) {
                return Ok(uuid);
            }
            return Err(AegisError::AgentNotFound { agent_id: uuid });
        }

        let matches: Vec<Uuid> = agents
            .iter()
            .filter(|agent| agent.agent_id.to_string().starts_with(raw))
            .map(|agent| agent.agent_id)
            .collect();

        match matches.as_slice() {
            [agent_id] => Ok(*agent_id),
            [] => Err(AegisError::IpcProtocol {
                reason: format!("Unknown agent_id prefix `{raw}`"),
            }),
            _ => Err(AegisError::IpcProtocol {
                reason: format!("Ambiguous agent_id prefix `{raw}`"),
            }),
        }
    }

    async fn nudge(&self, agent: &Agent) -> Result<bool> {
        if agent.status != AgentStatus::Active {
            return Ok(false);
        }

        let Some(tmux) = &self.tmux else {
            return Ok(false);
        };

        let target =
            TmuxTarget::parse(&agent.tmux_target()).map_err(|e| AegisError::IpcProtocol {
                reason: e.to_string(),
            })?;
        let command = format!("aegis message inbox {}", agent.agent_id);
        append_tmux_send(&self.storage.agent_log_path(agent.agent_id), &command)?;

        tmux.send_text(&target, &command)
            .await
            .map_err(|e| AegisError::IpcConnection {
                source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
            })?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::FileRegistry;
    use crate::storage::ProjectStorage;
    use aegis_core::{AgentKind, AgentRegistry, AgentStatus};
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn write_minimal_config(project_root: &std::path::Path) {
        let config = r#"
[providers.claude-code]
binary = "claude-code"

[splinter_defaults]
cli_provider = "claude-code"
"#;
        std::fs::write(project_root.join("aegis.toml"), config).unwrap();
    }

    fn test_agent(agent_id: Uuid, name: &str, status: AgentStatus) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id,
            name: name.to_string(),
            kind: AgentKind::Splinter,
            status,
            role: "splinter".to_string(),
            parent_id: None,
            task_id: None,
            tmux_session: "aegis".to_string(),
            tmux_window: 0,
            tmux_pane: "%0".to_string(),
            worktree_path: "/tmp/worktree".into(),
            cli_provider: "claude-code".to_string(),
            fallback_cascade: vec!["codex".to_string()],
            sandbox_profile: "/tmp/profile.sb".into(),
            log_path: "/tmp/log.log".into(),
            created_at: now,
            updated_at: now,
            terminated_at: None,
        }
    }

    fn router(project_root: &std::path::Path) -> (MessageRouter, Uuid) {
        write_minimal_config(project_root);
        let storage = Arc::new(ProjectStorage::new(project_root.to_path_buf()));
        storage.ensure_layout().unwrap();
        FileRegistry::init(storage.as_ref()).unwrap();
        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let agent_id = Uuid::parse_str("603685e0-1111-2222-3333-444444444444").unwrap();
        AgentRegistry::insert(
            registry.as_ref(),
            &test_agent(agent_id, "agent-one", AgentStatus::Active),
        )
        .unwrap();

        (MessageRouter::new(registry, storage, None), agent_id)
    }

    #[tokio::test]
    async fn send_writes_to_mailbox_and_lists_message() {
        let dir = tempdir().unwrap();
        let (router, agent_id) = router(dir.path());

        let receipt = router
            .send(
                None,
                &agent_id.to_string(),
                MessageType::Notification,
                serde_json::json!({"text": "hello"}),
            )
            .await
            .unwrap();

        assert_eq!(receipt.to_agent_id, agent_id);
        assert!(receipt.inbox_path.exists());

        let inbox = router.inbox(&agent_id.to_string()).unwrap();
        assert_eq!(inbox.messages.len(), 1);
        assert_eq!(
            inbox.messages[0].payload,
            serde_json::json!({"text": "hello"})
        );
    }

    #[test]
    fn list_summarizes_all_agent_inboxes() {
        let dir = tempdir().unwrap();
        let (router, agent_id) = router(dir.path());
        let mailbox = MailboxChannel::new(
            format!("mailbox-{agent_id}"),
            router.storage.agent_inbox_path(agent_id),
        );
        let message = Message {
            message_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            from: MessageSource::System,
            to_agent_id: agent_id,
            kind: MessageType::Notification,
            priority: 0,
            payload: serde_json::json!({"text":"queued"}),
            created_at: Utc.timestamp_opt(1, 0).single().unwrap(),
        };
        std::fs::create_dir_all(mailbox.inbox_path()).unwrap();
        std::fs::write(
            mailbox.message_path(&message),
            serde_json::to_string(&message).unwrap(),
        )
        .unwrap();

        let summaries = router.list().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].queued_messages, 1);
    }
}
