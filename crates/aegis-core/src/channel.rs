use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Injection,
    Mailbox,
    Observation,
    Broadcast,
    Telegram,
}

impl ChannelKind {
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::Injection | Self::Observation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Task,
    Handoff,
    Notification,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    Agent(Uuid),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub from: MessageSource,
    pub to_agent_id: Uuid,
    pub kind: MessageType,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(
        from: MessageSource,
        to_agent_id: Uuid,
        kind: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            from,
            to_agent_id,
            kind,
            priority: 0,
            payload,
            created_at: Utc::now(),
        }
    }
}

#[async_trait::async_trait]
pub trait Channel: Send + Sync {
    fn kind(&self) -> ChannelKind;
    fn name(&self) -> &str;
    fn is_active(&self) -> bool;
    async fn send(&self, message: &Message) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRecord {
    pub name: String,
    pub kind: ChannelKind,
    pub active: bool,
    pub registered_at: DateTime<Utc>,
    pub config: serde_json::Value,
}

pub trait ChannelRegistry: Send + Sync {
    fn register(&self, name: &str, kind: ChannelKind) -> Result<()>;
    fn deregister(&self, name: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<ChannelRecord>>;
    fn list(&self) -> Result<Vec<ChannelRecord>>;
}
