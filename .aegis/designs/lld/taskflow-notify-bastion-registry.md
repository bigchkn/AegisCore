# LLD-D: `aegis taskflow notify` & Bastion Registry

## Purpose

Defines how an external actor (human or another agent) signals the running bastion that the roadmap has changed, so it wakes from its EXHAUSTED idle state and re-evaluates the backlog.

---

## Bastion Registry

The daemon needs to know which agent is "the active bastion" to route notifications.

**Approach**: Use the existing agent registry (the agent list in controller state) filtered by role. An agent spawned from the `taskflow-bastion` template has `role = "bastion"`. The `taskflow.notify` IPC handler queries state for agents matching `role == "bastion" && status == Active`, takes the first result, and routes a message to it via the existing `MessageRouter`.

No new persistent data structure is needed. The role name is the routing key.

Edge case: if multiple bastion agents are active (e.g., old one not cleaned up), send to all of them — each will independently re-evaluate `aegis taskflow next` and the one without pending work will simply find nothing and return to idle.

---

## `aegis taskflow notify` CLI

```
aegis taskflow notify [--event <event>] [--message <text>]
```

Defaults: `--event roadmap_updated`, `--message "Roadmap updated externally."`

Sends IPC `taskflow.notify`:
```json
{ "event": "roadmap_updated", "message": "Roadmap updated externally." }
```

Response: `{"notified": 1}` (count of bastions messaged), or `{"notified": 0}` if no active bastion (not an error — it's a no-op if no bastion is running).

---

## IPC Handler: `taskflow.notify`

In `crates/aegis-controller/src/daemon/uds.rs`:

```
1. Deserialize params: { event: String, message: String }
2. Query agents: filter state for role="bastion", status=Active
3. For each: MessageRouter::send(from=System, to=agent_id, kind=Notification, body={"event":..., "message":...})
4. Return { "notified": <count> }
```

`from=System` requires a sentinel sender identity. Options:
- Use a well-known UUID `00000000-0000-0000-0000-000000000000` as the "system" sender.
- Or add a `MessageSender::System` variant to `MessageRouter`. Prefer the latter for clarity.

New variant:
```rust
pub enum MessageSender {
    Agent(Uuid),
    System,
}
```

`MessageRouter::send` signature changes to accept `MessageSender` for the `from` field.

---

## Bastion Template: Notification Handling

The bastion's system prompt instructs it:

> When in idle/exhausted state, check your inbox every 30 seconds. If you receive a notification with `{"event":"roadmap_updated"}`, immediately call `aegis taskflow next` and re-enter the work loop if a milestone is available.

The bastion uses `aegis message list --unread` to check its inbox. It acknowledges (marks read) after acting on the notification.

---

## Agent-to-Agent Notification Path

An agent (e.g., a splinter that adds a new task to the roadmap) can also call:
```
aegis taskflow notify
```
directly from its shell environment. This is equivalent to a human running the same command — it goes through the daemon IPC path and routes to the bastion.

---

## File Changes Summary

| File | Change |
|------|--------|
| `crates/aegis-controller/src/messaging.rs` | Add `MessageSender` enum; update `send()` signature |
| `crates/aegis-controller/src/daemon/uds.rs` | Add `taskflow.notify` handler |
| `crates/aegis-controller/src/commands.rs` | Add `taskflow_notify(event, message)` |
| `src/commands/taskflow.rs` | Add `notify()` function |
| `src/main.rs` | Add `Notify` variant to `TaskflowCommands` |

---

## Test Cases

- `notify_with_active_bastion_sends_message` — mock state with one active bastion-role agent; assert message enqueued
- `notify_with_no_active_bastion_returns_zero` — no bastion in state; assert `{"notified": 0}`, no error
- `notify_routes_to_all_active_bastions` — two active bastion agents; assert both receive message
