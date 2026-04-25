# LLD: Web Agent Controls

**Milestone:** M15  
**Status:** draft  
**HLD ref:** §18  
**Implements:** `crates/aegis-web/`  
**Depends on:** M11 (HTTP/WebSocket daemon), M14 protocol surfaces

---

## 1. Purpose

The web UI already renders agent state, live panes, logs, tasks, and channels. This LLD covers the missing operator control surface for agent creation and spawning:

- a visible composer for starting a new agent from the Agents view
- a WebSocket/API-backed submit flow that uses the existing controller `spawn` command
- UX feedback for loading, validation, and failure states
- tests that verify the form remains available even when no agents are active

The backend does not expose a distinct "create agent" verb for the web UI. In this implementation, the operator creates a new agent by submitting a task prompt to the existing `spawn` command, which instantiates the agent and dispatches it through the controller.

---

## 2. Web Surface

### 2.1 Agents View Composer

The `AgentsView` always renders a compact composer above the agent table or empty state. The composer contains:

- a multiline prompt field
- a primary `Spawn Agent` submit button
- inline validation for empty prompts
- inline error text for failed dispatches

The composer is intentionally persistent so operators can spawn a new agent even when the project currently has zero active agents.

### 2.2 Interaction Flow

1. User selects a project.
2. User enters a task prompt in the Agents composer.
3. User submits the form.
4. The frontend dispatches the existing `spawnTask` thunk.
5. The controller enqueues a task and returns the new registry task id.
6. Websocket state updates repopulate the Agents table when the spawned agent registers.

---

## 3. API Contract

### 3.1 Existing Command

The implementation reuses the existing controller command:

```json
POST /projects/:id/commands
{
  "command": "spawn",
  "params": "Investigate the controller queue"
}
```

### 3.2 Frontend Thunk

The React slice uses the existing `spawnTask` thunk in `crates/aegis-web/frontend/src/api/thunks.ts`.

No new backend route is required for this milestone.

---

## 4. Taskflow CLI Support

To keep roadmap progress manageable from the CLI, Taskflow gains a task-status command:

- `aegis taskflow set-task-status <M-ID> <TASK-ID> <STATUS>`

The command updates the milestone fragment and the index status through the daemon, using the same file-locking discipline as the existing create/add commands.

---

## 5. Test Strategy

| Test | Asserts |
|---|---|
| `AgentsView renders composer with no agents` | Spawn control is visible even when the agent table is empty |
| `AgentsView submits spawn payload` | Form submit dispatches the existing spawn command with the entered prompt |
| `taskflow set-task-status` | Task status updates the milestone fragment and index record |
