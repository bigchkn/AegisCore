# LLD: Agent Attach Persistence

**Milestone:** M28  
**Status:** draft  
**HLD ref:** [hld/attach.md](../hld/attach.md) §7.4, §11

## 1. Purpose

Provide a seamless "resume" experience for operators by persisting the last attached agent identity across daemon restarts and UI sessions. This ensures that when the Aegis daemon or a UI client (TUI/Web) restarts, the system can automatically restore focus to the previously active agent if that agent is still alive.

## 2. State Model

Persistence will be handled at the **Project** level rather than the **Agent** level, as attachment is a property of the operator's focus within a specific project context.

### 2.1 Project Registry Update

Extend `ProjectRecord` in `crates/aegis-controller/src/daemon/projects.rs`:

```rust
pub struct ProjectRecord {
    pub id: Uuid,
    pub root_path: PathBuf,
    pub auto_start: bool,
    pub last_seen: DateTime<Utc>,
    pub status: Option<String>,
    // New field
    pub last_attached_agent_id: Option<Uuid>,
}
```

This field will be persisted in `~/.aegis/state/projects.json`.

## 3. Daemon Implementation

### 3.1 IPC Handler Updates

The daemon must update the project record whenever an attachment is successfully initiated.

**UDS (`pane.attach`):**
In `handle_pane_attach` (uds.rs), after resolving the `agent_id`, call `project_registry.update_last_attached(project_id, Some(agent_id))`.

**WebSocket (`/ws/pane/:agent_id`):**
In the WebSocket handler (http.rs), update the project record when a terminal session starts.

### 3.2 Cleanup on Termination

If an agent terminates (Normal, Failed, or Killed), the `last_attached_agent_id` for that project should be cleared if it matches the terminated agent.

Logic should be added to `Dispatcher::cleanup_agent` or the status transition monitor to ensure we don't try to auto-attach to a dead agent on next boot.

## 4. UI Implementation

### 4.1 TUI Auto-Attach

1.  **Startup Sequence**: On boot, the TUI calls `project.status` or `agents.list`.
2.  **Resolution**: The `project.status` response should be expanded to include `last_attached_agent_id`.
3.  **Auto-Attach**: If `last_attached_agent_id` is present AND the corresponding agent is in an `Active` or `Paused` state:
    *   Set `app.attached_agent_id`.
    *   Set `app.mode = PaneMode::Input`.
    *   Initiate the UDS `pane.attach` stream automatically.

### 4.2 Web UI Restoration

1.  The Redux `project` slice should include `lastAttachedAgentId`.
2.  On initial load, if the URL does not specify a specific agent route (e.g., just `/project/:id`), the frontend should redirect or default its view to the `PaneView` of the persisted agent.

## 5. IPC API Changes

### 5.1 `project.status` / `status`

Add `last_attached_agent_id` to the JSON payload:

```json
{
  "project_root": "...",
  "agents": { ... },
  "last_attached_agent_id": "uuid-here"
}
```

### 5.2 `project.update_attach` (Internal/Optional)

A dedicated IPC command for UIs to manually update focus without opening a full stream, though implicit update on attach is preferred.

## 6. Test Strategy

| Test | Asserts |
|---|---|
| `test_persistence_on_attach` | Attaching via UDS updates `projects.json` |
| `test_cleanup_on_termination` | Terminating the attached agent clears the field in `projects.json` |
| `test_tui_auto_reconnect` | TUI mock client enters input mode if project has an active attached agent |
| `test_restart_cycle` | Stop daemon → Start daemon → `project.status` still returns the ID |
