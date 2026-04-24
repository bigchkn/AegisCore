use aegis_core::AegisEvent;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AegisEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: AegisEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AegisEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::AgentStatus;
    use uuid::Uuid;

    #[tokio::test]
    async fn publish_subscribe_delivers_events() {
        let bus = EventBus::new(8);
        let mut rx = bus.subscribe();
        let agent_id = Uuid::new_v4();

        bus.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status: AgentStatus::Starting,
            new_status: AgentStatus::Active,
        });

        let event = rx.recv().await.unwrap();
        match event {
            AegisEvent::AgentStatusChanged {
                agent_id: received,
                old_status,
                new_status,
            } => {
                assert_eq!(received, agent_id);
                assert_eq!(old_status, AgentStatus::Starting);
                assert_eq!(new_status, AgentStatus::Active);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
