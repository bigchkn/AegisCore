use crate::mailbox::MailboxChannel;
use aegis_core::{AgentRegistry, Channel, ChannelKind, Message, Result, StorageBackend};
use async_trait::async_trait;
use std::sync::Arc;

pub struct BroadcastChannel {
    name: String,
    registry: Arc<dyn AgentRegistry>,
    storage: Arc<dyn StorageBackend>,
}

impl BroadcastChannel {
    pub fn new(
        name: String,
        registry: Arc<dyn AgentRegistry>,
        storage: Arc<dyn StorageBackend>,
    ) -> Self {
        Self {
            name,
            registry,
            storage,
        }
    }
}

#[async_trait]
impl Channel for BroadcastChannel {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Broadcast
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_active(&self) -> bool {
        true
    }

    async fn send(&self, message: &Message) -> Result<()> {
        let active_agents = self.registry.list_active()?;

        for agent in active_agents {
            let mut agent_msg = message.clone();
            agent_msg.to_agent_id = agent.agent_id;

            let mailbox = MailboxChannel::new(
                format!("mailbox-{}", agent.agent_id),
                self.storage.agent_inbox_path(agent.agent_id),
            );
            mailbox.send(&agent_msg).await?;
        }
        Ok(())
    }
}
