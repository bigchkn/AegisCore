# LLD-A: Continuous Bastion State Machine

## Purpose

Defines the bastion agent's full execution loop: the state machine it traverses, how it picks milestones, how it coordinates parallel splinters, how it handles retries and escalation, and how it idles until notified of new work.

---

## State Machine

```
             ┌──────────────────────────────────────────────────────────────┐
             ▼                                                              │
         [IDLE / EXHAUSTED]                                                │
             │                                                              │
             │  aegis taskflow next returns a milestone                     │
             ▼                                                              │
         [PICK_MILESTONE]                                                   │
             │                                                              │
             │  create worktree for milestone branch                        │
             ▼                                                              │
         [SPAWN_SPLINTERS]                                                  │
             │  spawn one splinter per pending task (parallel)              │
             ▼                                                              │
         [AWAIT_COMPLETION]                ◄──── retry loop (max 3)        │
             │  poll each splinter inbox                                    │
             │                                                              │
             ├── all done ──► [MERGE] ─── success ──────────────────────────┘
             │                          └── conflict → [CLARIFY]
             │
             └── any blocked after 3 retries ──► [CLARIFY]
```

States:
- **IDLE / EXHAUSTED**: No pending milestones. Bastion polls `aegis taskflow next` on a short interval (30s). On receiving a `notification` message (event `roadmap_updated`), immediately re-enters PICK_MILESTONE.
- **PICK_MILESTONE**: Calls `aegis taskflow next`, reads the milestone ID and tasks from `aegis taskflow show <id>`.
- **SPAWN_SPLINTERS**: Calls `aegis worktree create milestone/<id>` (if not already present from resume). Spawns one splinter per pending task in parallel via `aegis design spawn taskflow-splinter`. Sends each splinter a `task` message with `{"task_id": "...", "task_description": "..."}`.
- **AWAIT_COMPLETION**: Polls each splinter's response by reading its inbox (via `aegis message list --from <splinter_id>`). Maintains a per-task attempt counter.
- **MERGE**: Calls `aegis worktree merge milestone/<id>`. Marks milestone `done` in taskflow (`aegis taskflow sync`). Loops back to PICK_MILESTONE.
- **CLARIFY**: Sends a human clarification request (`aegis clarify ask`) describing what is blocked. Waits for response. On resume, retries the blocked task from attempt 0 (human intervention resets the counter), or marks it skipped if the human says so.

---

## Polling Interval & Notification

When EXHAUSTED:
1. Poll `aegis taskflow next` every 30 seconds.
2. Also continuously read inbox. If a `notification` message arrives with body `{"event":"roadmap_updated"}`, skip the sleep and re-enter PICK_MILESTONE immediately.

When AWAIT_COMPLETION:
1. Poll each splinter inbox every 10 seconds.
2. A splinter reports `{"status":"done"}` or `{"status":"blocked","reason":"..."}`.

---

## Retry Logic

Per-task attempt counter (not persisted, reset on milestone restart):
- Attempt 1, 2, 3: Kill old splinter (if still alive), spawn fresh splinter, re-send task message.
- Attempt > 3: Enter CLARIFY state for that task.

A splinter is considered "stuck" (treated as blocked) if no response arrives within 10 minutes of last message.

---

## Parallel Splinter Coordination

The bastion spawns all splinters for a milestone in a tight loop before waiting. It does not wait for splinter N to finish before spawning splinter N+1. It maintains a map:

```
task_id → (splinter_agent_id, attempt_count, status)
```

AWAIT_COMPLETION resolves when every task_id has `status = done`.

Important: all splinters share one milestone worktree. The bastion must assign tasks such that they touch non-overlapping files where possible (this is a prompt-level instruction, not enforced by the system). If a merge conflict occurs in the milestone worktree during `aegis worktree merge`, the bastion enters CLARIFY.

---

## Resume Protocol

On startup, the bastion runs:
1. `aegis taskflow status` — find any milestone with status `in-progress`.
2. If found, skip PICK_MILESTONE and go directly to SPAWN_SPLINTERS for the in-progress milestone (only spawn splinters for tasks with status `pending` or `in-progress`, skip `done`).
3. Check inbox for any existing splinter responses that arrived while the bastion was down.

---

## Template Changes Required

The `taskflow-bastion` system prompt and startup in `crates/aegis-design/src/builtin/taskflow-bastion/` need to be rewritten to describe the full loop. Key changes:
- Remove single-milestone framing; replace with loop framing.
- Add `aegis taskflow next` and `aegis worktree create/merge` to the tools list.
- Add retry counter instructions.
- Add idle/notification wait instructions.
- Startup: check resume state first, then enter loop.

---

## New Commands Required

| Command | Purpose |
|---------|---------|
| `aegis taskflow next` | Returns next milestone ID to work on (see LLD-C) |
| `aegis worktree create <branch>` | Creates milestone git worktree (see LLD-B) |
| `aegis worktree merge <branch>` | Merges milestone worktree into main (see LLD-B) |
| `aegis taskflow notify` | Sends `roadmap_updated` event to active bastion (see LLD-D) |
