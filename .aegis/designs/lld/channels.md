# LLD: `aegis-channels`

**Milestone:** M6  
**Status:** in-progress  
**HLD ref:** §4, §14.5  
**Implements:** `crates/aegis-channels/` — Injection, Mailbox, and Broadcast channels; Channel Registry.

---

## 1. Purpose

`aegis-channels` provides the communication backbone between the Controller and agents, and between agents themselves. It implements the `Channel` and `ChannelRegistry` traits defined in `aegis-core`.

**Key Capabilities:**
- **Injection:** Low-latency command/text injection via tmux `send-keys`.
- **Mailbox:** Asynchronous, persistent message delivery via the filesystem.
- **Broadcast:** Fan-out delivery to all active agents.
- **Registry:** Persistence of explicit channel configurations (Mailbox, Telegram).

---

## 2. Module Structure

```
crates/aegis-channels/
├── Cargo.toml
└── src/
    ├── lib.rs              ← Re-exports; Registry factory
    ├── injection.rs        ← InjectionChannel implementation
    ├── mailbox.rs          ← MailboxChannel implementation
    ├── observation.rs      ← Observation service (tmux capture-pane)
    ├── broadcast.rs        ← BroadcastChannel implementation
    ├── registry.rs         ← FileChannelRegistry implementation
    └── error.rs            ← Channel-specific errors
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core = { path = "../aegis-core" }
aegis-tmux = { path = "../aegis-tmux" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4" }
fs2 = "0.4"
tempfile = "3"
async-trait = "0.1"
tracing = "0.1"
```

---

## 4. `InjectionChannel`

Implicit channel that wraps `aegis-tmux` to deliver text/commands to an agent's terminal.

### 4.1 Implementation

```rust
pub struct InjectionChannel {
    name: String,
    tmux: Arc<TmuxClient>,
    target: TmuxTarget,
}

impl Channel for InjectionChannel {
    fn kind(&self) -> ChannelKind { ChannelKind::Injection }
    fn name(&self) -> &str { &self.name }
    fn is_active(&self) -> bool { true }

    fn send(&self, message: &Message) -> Result<()> {
        // Message payload for Injection is expected to be a string or JSON-wrapped string
        let text = match &message.payload {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // Fire-and-forget; block_on if trait is sync, or use async-trait
        self.tmux.send_text(&self.target, &text)?;
        Ok(())
    }
}
```

**Note:** The `Channel` trait in `aegis-core` is currently sync. Implementation will use `tokio::runtime::Handle::current().block_on()` if needed, or we will update `aegis-core` to use `async-trait`. Given the project uses `tokio`, `async-trait` is preferred.

---

## 5. `MailboxChannel`

Explicit channel using the filesystem as a persistent buffer. Each agent has an inbox at `.aegis/channels/<agent_id>/inbox/`.

### 5.1 Inbox Layout
```
.aegis/channels/<agent_id>/inbox/
├── <timestamp>_<uuid>.json
└── ...
```

### 5.2 Implementation

```rust
pub struct MailboxChannel {
    name: String,
    inbox_path: PathBuf,
}

impl Channel for MailboxChannel {
    fn kind(&self) -> ChannelKind { ChannelKind::Mailbox }
    fn name(&self) -> &str { &self.name }
    fn is_active(&self) -> bool { self.inbox_path.exists() }

    fn send(&self, message: &Message) -> Result<()> {
        let filename = format!("{}_{}.json", message.created_at.timestamp(), message.message_id);
        let path = self.inbox_path.join(filename);

        // Atomic write via tempfile
        let mut tmp = tempfile::NamedTempFile::new_in(&self.inbox_path)?;
        serde_json::to_writer_pretty(&mut tmp, message)?;
        tmp.persist(path)?;
        
        Ok(())
    }
}
```

---

## 6. `BroadcastChannel`

Funnels a single message to multiple `MailboxChannel` instances.

### 6.1 Implementation

```rust
pub struct BroadcastChannel {
    name: String,
    registry: Arc<dyn AgentRegistry>,
    storage: Arc<dyn StorageBackend>,
}

impl Channel for BroadcastChannel {
    fn kind(&self) -> ChannelKind { ChannelKind::Broadcast }
    fn name(&self) -> &str { &self.name }
    fn is_active(&self) -> bool { true }

    fn send(&self, message: &Message) -> Result<()> {
        let active_agents = self.registry.list_active()?;
        
        for agent in active_agents {
            let mut agent_msg = message.clone();
            agent_msg.to_agent_id = agent.agent_id;
            
            let mailbox = MailboxChannel {
                name: format!("mailbox-{}", agent.agent_id),
                inbox_path: self.storage.agent_inbox_path(agent.agent_id),
            };
            mailbox.send(&agent_msg)?;
        }
        Ok(())
    }
}
```

---

## 7. `FileChannelRegistry`

Persists explicit channel configurations to `.aegis/state/channels.json`.

### 7.1 Schema (`channels.json`)
```json
{
  "version": 1,
  "channels": [
    {
      "name": "telegram",
      "kind": "telegram",
      "active": true,
      "registered_at": "...",
      "config": { "token_env": "...", "allowed_chat_ids": [...] }
    }
  ]
}
```

### 7.2 Concurrency
Uses `fs2` advisory locking on `channels.json` for all read/write operations, following the `LockedFile` pattern from `aegis-controller`.

---

## 8. Observation Service

While `Observation` is a `ChannelKind`, it behaves as a pull-based service rather than a push-based `Channel`.

```rust
pub struct ObservationService {
    tmux: Arc<TmuxClient>,
}

impl ObservationService {
    /// Scrape the last N lines from an agent's pane.
    pub async fn scrape(&self, agent: &Agent, lines: usize) -> Result<String> {
        let target = TmuxTarget::new(&agent.tmux_session, agent.tmux_window, &agent.tmux_pane);
        self.tmux.capture_pane_plain(&target, lines).await
            .map_err(Into::into)
    }
}
```

---

## 9. Integration with `aegis-core`

We will update `aegis-core/src/channel.rs` to use `#[async_trait]` to avoid blocking the executor in `InjectionChannel`.

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn send(&self, message: &Message) -> Result<()>;
    // ...
}
```

---

## 10. Test Strategy

### 10.1 Unit Tests
- `test_mailbox_atomic_write`: Verify messages are written as JSON and persistent.
- `test_broadcast_fanout`: Mock `AgentRegistry` and verify message appears in multiple inbox directories.
- `test_registry_persistence`: Register a channel, reload registry, and verify it exists.

### 10.2 Integration Tests
- `test_injection_to_tmux`: Verify `InjectionChannel` actually sends keys to a real tmux session (requires `aegis-tmux` helpers).
