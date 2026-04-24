# LLD — Telegram Bridge (`aegis-telegram`)

**Status:** Draft v1.0  
**Crate:** `crates/aegis-telegram`  
**Depends on:** `aegis-core`

---

## 1. Introduction

The Telegram Bridge provides a remote human-in-the-loop interface for AegisCore. It allows users to monitor agent status, receive notifications for critical events (rate limits, task completion), and issue manual override commands from any authorized Telegram client.

## 2. Architecture

The bridge runs as an asynchronous task within the AegisCore daemon (`aegisd`). It leverages the `teloxide` framework for robust Telegram Bot API interaction.

### 2.1 Core Components

- **`TelegramBridge`**: The main entry point that manages the bot lifecycle (long-polling or webhooks).
- **`CommandParser`**: Orchestrates the transformation of inbound Telegram messages into AegisCore commands.
- **`EventPublisher`**: Listens to the internal AegisCore event bus and fan-outs formatted notifications to authorized chats.
- **`SecurityGuard`**: Enforces the Chat ID allowlist for all inbound requests.

### 2.2 Connectivity

- **Outbound**: `HTTPS` to `api.telegram.org:443`.
- **Inbound (Long Polling)**: Bot pulls updates from Telegram.
- **Inbound (Webhook)**: AegisCore exposes an endpoint (optional, v2).

## 3. Data Structures

### 3.1 Configuration (`TelegramConfig`)

```rust
pub struct TelegramConfig {
    pub token: String,               // Bot API Token (usually from ENV)
    pub allowed_chat_ids: Vec<i64>,  // Authorized Telegram Chat IDs
    pub enabled: bool,
    pub notify_on: Vec<EventType>,   // Events that trigger a message
}
```

### 3.2 Internal State

```rust
pub struct TelegramState {
    pub bot: Bot,
    pub config: TelegramConfig,
    // Channel for receiving internal AegisCore events
    pub event_rx: mpsc::Receiver<AegisEvent>,
}
```

## 4. Inbound Command Parser

Supported commands and their mappings to AegisCore internal logic:

| Command | Args | Action |
|---|---|---|
| `/start` | - | Return greeting and Chat ID (for setup) |
| `/status` | - | Return high-level summary of active projects and global daemon health |
| `/agents` | - | List all agents across all projects with status and provider |
| `/pause` | `<agent_id>` | Call `dispatcher.pause_agent(agent_id)` |
| `/resume` | `<agent_id>` | Call `dispatcher.resume_agent(agent_id)` |
| `/kill` | `<agent_id>` | Call `dispatcher.kill_agent(agent_id)` |
| `/spawn` | `<role> <task>` | Call `dispatcher.spawn_splinter(role, task)` |
| `/logs` | `<agent_id> [n]` | Fetch last N lines from `FlightRecorder` and send as message/file |
| `/failover`| `<agent_id>` | Manually trigger `watchdog.trigger_failover(agent_id)` |

### 4.1 Command Handling Logic

1. **Authorize**: Check `message.chat.id` against `allowed_chat_ids`.
2. **Log**: Record command attempt (redacted) in AegisCore logs.
3. **Dispatch**: Map to internal Controller method.
4. **Respond**: Send MarkdownV2 formatted confirmation or error message.

## 5. Outbound Event Publisher

The `EventPublisher` task monitors the global event bus.

### 5.1 Event Mapping

| Event Type | Telegram Message Template |
|---|---|
| `AgentSpawned` | "🚀 **Splinter Spawned**\nID: `{{id}}`\nRole: `{{role}}`" |
| `TaskComplete` | "✅ **Task Complete**\nTask: `{{task}}`\nReceipt: `{{path}}`" |
| `RateLimitDetected`| "⚠️ **Rate Limit**\nAgent: `{{name}}`\nProvider: `{{provider}}`\nAction: Failover initiated" |
| `SandboxViolation` | "🚫 **Sandbox Violation**\nAgent: `{{id}}`\nPath: `{{path}}`" |
| `AgentFailed` | "💀 **Agent Crashed**\nID: `{{id}}`\nExit Code: `{{code}}`" |

### 5.2 Rate Limiting

To avoid Telegram API 429 errors during "event storms" (e.g., many splinters finishing at once):
- **Buffer**: Use a small internal buffer for events.
- **Throttling**: Max 30 messages per second (Telegram's global limit) or 1 message per second per chat.
- **Aggregation**: If >5 events of the same type occur within 2 seconds, send a single summary message ("5 Splinters spawned").

## 6. Security & Privacy

- **Credential Storage**: The Telegram Token MUST NEVER be stored in `aegis.toml`. It should be read from the `AEGIS_TELEGRAM_TOKEN` environment variable or a secure keychain.
- **ID Validation**: Silently ignore any message from a Chat ID not in the allowlist.
- **Input Sanitization**: All user-provided strings (tasks, roles) must be escaped before being passed to shell commands or stored.
- **Markdown Escaping**: Ensure all dynamic content is escaped for Telegram MarkdownV2 to prevent rendering errors.

## 7. Implementation Plan (M9)

1. **Task 9.2**: Integrate `teloxide` and implement the basic long-poll loop.
2. **Task 9.3**: Implement `SecurityGuard` middleware for ID allowlist.
3. **Task 9.4**: Implement `CommandParser` with basic `/status` and `/agents` commands.
4. **Task 9.5**: Implement `EventPublisher` with `tokio` mpsc integration.
5. **Task 9.6**: Integrate with `aegis-controller` for real command execution.
6. **Task 9.7**: Mock Telegram API for integration tests.

## 8. Dependencies

- `teloxide`: Bot framework.
- `tokio`: Async runtime.
- `serde`: Serialization for config/events.
- `handlebars` or simple `replace`: For message templates.
