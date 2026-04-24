use aegis_core::{Channel, ChannelKind, Message, Result};
use aegis_tmux::{TmuxClient, TmuxTarget};
use async_trait::async_trait;
use std::sync::Arc;

pub struct InjectionChannel {
    name: String,
    tmux: Arc<TmuxClient>,
    target: TmuxTarget,
}

impl InjectionChannel {
    pub fn new(name: String, tmux: Arc<TmuxClient>, target: TmuxTarget) -> Self {
        Self { name, tmux, target }
    }
}

#[async_trait]
impl Channel for InjectionChannel {
    fn kind(&self) -> ChannelKind {
        ChannelKind::Injection
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_active(&self) -> bool {
        true
    }

    async fn send(&self, message: &Message) -> Result<()> {
        let text = match &message.payload {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        self.tmux
            .send_text(&self.target, &text)
            .await
            .map_err(|e| aegis_core::error::AegisError::from(e))?;

        Ok(())
    }
}
