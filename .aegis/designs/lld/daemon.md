# LLD: Global Daemon & IPC (`aegisd`)

**Milestone:** M11  
**Status:** draft  
**HLD ref:** §2.1, §4.6, §11, §12.5  
**Depends on:** M10 (Controller & Dispatcher)

---

## 1. Purpose

`aegisd` is the central orchestrator process for AegisCore. It runs as a background daemon on macOS, managing the lifecycle of multiple projects simultaneously. It provides three primary interfaces:
1. **Unix Domain Socket (UDS):** High-performance, local Request/Response + Event Stream for the `aegis` CLI.
2. **HTTP REST API:** Management interface for the Web UI and remote tools.
3. **WebSocket API:** Real-time event subscription for the TUI and Web UI.

---

## 2. Architecture

The daemon acts as a container for multiple `AegisRuntime` instances (defined in M10).

```
aegisd (Process)
├── Project Registry (~/.aegis/projects.json)
├── Runtime Manager
│   ├── Project A (AegisRuntime)
│   ├── Project B (AegisRuntime)
│   └── ...
├── IPC Servers
│   ├── UDS Server (/tmp/aegis.sock)
│   ├── HTTP Server (:7437)
│   └── WebSocket Server (/ws/events)
└── macOS Integration (launchd)
```

### 2.1 Project Isolation

Each project managed by the daemon is identified by its absolute path to the `.aegis/` directory. The daemon ensures that:
- Only one `AegisRuntime` exists per project path.
- Runtimes are loaded on-demand (e.g., when a CLI command is issued for that project).
- Runtimes are persisted in a global registry so they can be resumed after a daemon restart.

---

## 3. IPC: Unix Domain Socket

Primary interface for the `aegis` CLI.

### 3.1 Socket Details
- **Path:** `/tmp/aegis.sock` (default; configurable via `~/.aegis/config`).
- **Permissions:** `0600` (restricted to the user who started the daemon).
- **Protocol:** Line-delimited JSON.

### 3.2 Request/Response Schema

**Request:**
```json
{
  "id": "uuid",
  "project_path": "/Users/user/project",
  "command": "spawn_agent",
  "params": {
    "name": "architect",
    "role": "architect"
  }
}
```

**Response:**
```json
{
  "id": "uuid",
  "status": "success | error",
  "payload": { ... },
  "error": null | { "code": "...", "message": "..." }
}
```

### 3.3 Event Streaming

Clients can send a `subscribe` command to receive a continuous stream of `AegisEvent` objects (serialized as JSON) over the same socket.

---

## 4. IPC: HTTP & WebSocket

Provides rich data access for UIs.

### 4.1 HTTP REST Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects` | List all registered projects |
| `GET` | `/projects/:id/agents` | List active agents for a project |
| `GET` | `/projects/:id/tasks` | List pending/active tasks |
| `POST` | `/projects/:id/commands` | Dispatch a command to a project |
| `GET` | `/projects/:id/logs/:agent_id` | Stream/Tail logs for an agent |

### 4.2 WebSocket (`/ws/events`)

Real-time fan-out of `AegisEvent` notifications.
- Supports filtering by project ID or event type.
- Used by `aegis-tui` and `aegis-web`.

---

## 5. Project Registry

Managed at the user level (outside any specific project).

- **Path:** `~/.aegis/state/projects.json`
- **Format:**
```json
{
  "projects": [
    {
      "id": "uuid",
      "root_path": "/Users/user/project-a",
      "auto_start": true,
      "last_seen": "iso8601"
    }
  ]
}
```

The daemon reads this on startup to re-initialize active projects.

---

## 6. MacOS Integration (`launchd`)

`aegisd` includes a command to generate and install its own `launchd` plist.

- **Label:** `com.aegiscore.aegisd`
- **Path:** `~/Library/LaunchAgents/com.aegiscore.aegisd.plist`
- **RunAtLoad:** `true`
- **KeepAlive:** `true`

---

## 7. Startup & Shutdown Lifecycle

### 7.1 Startup Sequence
1. Bind Unix Socket and HTTP Server.
2. Load Project Registry from `~/.aegis/state/projects.json`.
3. For each `auto_start` project:
   - Perform `AegisRuntime::recover()`.
   - Start Watchdog and StateManager.
4. Signal readiness.

### 7.2 Shutdown Sequence (SIGTERM/SIGINT)
1. Stop accepting new IPC connections.
2. For each active project:
   - Snapshot state via `StateManager`.
   - Send `BroadcastChannel` shutdown notification to agents.
   - Wait for `MAX_SHUTDOWN_DRAIN_S` (default: 5s).
3. Close servers and exit.

---

## 8. Dependencies (`Cargo.toml` additions)

```toml
axum = { version = "0.7", features = ["ws"] }
tower-http = { version = "0.5", features = ["fs", "cors"] }
tokio-util = { version = "0.7", features = ["codec"] }
interprocess = "1.2"  # cross-platform UDS/NamedPipe support
```

---

## 9. Test Strategy

| Test | Description |
|---|---|
| `test_uds_request_response` | Verify CLI-style round-trip over UDS |
| `test_multi_project_routing` | Send commands to two different projects; verify isolation |
| `test_http_rest_api` | Verify REST responses match Registry state |
| `test_websocket_broadcast` | Trigger event in Runtime A; verify WS client receives it |
| `test_graceful_shutdown` | Verify state snapshots are written before process exits |
| `test_project_registration` | `aegis init` registers project; daemon picks it up immediately |
