# LLD: Agent-to-Agent Messaging

**Milestone:** M23  
**Status:** draft  
**HLD refs:** §4 Channels, §5 Agent lifecycle, §14 execution flow  
**Purpose:** Define the supported path for one agent to send a structured message to another agent in the same project.

---

## 1. Purpose

This document defines the message routing path for agent-to-agent communication in AegisCore.

The core requirement is simple:
- an agent can discover another agent's ID,
- address a message to that ID,
- and have the message delivered through the existing channel layer without relying on a new broker or a direct tmux-to-tmux link.

This is intentionally a **controller-mediated** path. tmux remains the execution plane, but tmux is not the messaging bus between peers.

---

## 2. Non-Goals

This milestone does not introduce:
- direct peer-to-peer tmux piping between agents,
- a distributed message broker,
- cross-project routing,
- host-wide agent discovery,
- or a new global pub/sub substrate.

The design stays inside the current project boundary and uses the current registry, storage, and channel primitives.

---

## 3. Design Summary

The messaging path is built from three existing ideas:

1. **Discovery**: the sender learns the target agent's `agent_id` from `aegis agents`.
2. **Addressing**: the message is addressed to that `agent_id`, not to a role name.
3. **Delivery**: the controller writes the message to the recipient's mailbox and optionally nudges the recipient through an injection prompt if it is active.

This makes messaging work for:
- Bastion -> Bastion
- Bastion -> Splinter
- Splinter -> Bastion
- Splinter -> Splinter

The delivery rules do not depend on the agent kind. A Bastion is just another registry entry with a mailbox destination.

---

## 4. Message Flow

### 4.1 Sender-side flow

1. The sender asks the controller for the current project agent list.
2. The sender picks a target `agent_id`.
3. The sender creates a message envelope with:
   - `from_agent_id`
   - `to_agent_id`
   - `kind`
   - `payload`
   - `priority`
   - `created_at`
4. The sender submits the message through the CLI or IPC layer.
5. The controller resolves the recipient and writes the message to the recipient inbox.

### 4.2 Delivery flow

1. The controller uses the recipient ID to locate `.aegis/channels/<agent_id>/inbox/`.
2. The message is written atomically as JSON.
3. If the target agent is active, the controller may inject a short notification prompt telling it to inspect its inbox.
4. If the target agent is inactive, the message remains queued until the agent returns.

### 4.3 Receipt flow

The agent reads its own inbox at a safe point in the loop, typically:
- at startup,
- after a task completes,
- after a pause/resume transition,
- or when explicitly prompted to inspect the inbox.

The message file is the source of truth. Delivery is durable once the JSON file exists.

---

## 5. Message Model

The current core channel model already gives us the right primitive:

```rust
pub struct Message {
    pub message_id: Uuid,
    pub from: MessageSource,
    pub to_agent_id: Uuid,
    pub kind: MessageType,
    pub priority: i32,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
```

For agent-to-agent messaging, the message kinds are:
- `task`
- `handoff`
- `notification`
- `command`

Payload is opaque JSON at the transport layer. The sender and receiver agree on the payload schema by convention.

Recommended payload patterns:
- `task`: task instructions and acceptance criteria
- `handoff`: context summary, current state, and next step
- `notification`: simple status updates or alerts
- `command`: explicit operational instructions

---

## 6. Routing Rules

### 6.1 Recipient resolution

The controller resolves the target agent by:
- exact UUID, or
- unique UUID prefix inside the current project.

This matches the existing controller ID resolution behavior and avoids forcing operators to type full UUIDs for every message.

### 6.2 Valid recipients

A message can target:
- an active agent,
- a paused agent,
- or a known non-terminal archived agent if the implementation explicitly allows queueing for later recovery.

The default behavior should be conservative:
- accept active and paused agents,
- queue messages to those inboxes,
- reject unknown IDs,
- reject ambiguous prefixes.

### 6.3 Ordering

Mailbox files are append-only and named with timestamp plus message UUID.
That gives:
- stable replay,
- deterministic sorting,
- and collision resistance.

No second delivery index is required for the first version.

### 6.4 Duplicate handling

The message UUID is the idempotency key.

If the same message is retried after a partial failure:
- a duplicate on disk should be detectable by UUID,
- and repeated writes should be treated as a transport retry problem, not as a new logical message.

---

## 7. Controller API Surface

The controller should expose a message router rather than making every caller talk to mailbox storage directly.

Recommended controller commands:
- `message.send`
- `message.inbox`
- `message.ack`
- `message.list`

Recommended CLI shape:
- `aegis message send <agent_id> <text>`
- `aegis message inbox [<agent_id>]`
- `aegis message list [<agent_id>]`

The sender does not need to know where the recipient mailbox lives. It only needs the controller-visible agent ID.

---

## 8. Delivery Semantics

### 8.1 Durable write first

Delivery is considered successful only after the mailbox file is written atomically.

### 8.2 Optional prompt nudge

If the recipient is active, the controller may send a lightweight injection message to tell the agent to inspect its inbox.

This is a convenience mechanism, not the durable delivery path.

### 8.3 Failure behavior

If mailbox write fails:
- return an error to the sender,
- do not inject a prompt,
- do not mark the message delivered.

If mailbox write succeeds but the prompt nudge fails:
- keep the queued message,
- surface a best-effort delivery warning,
- and let the next inbox poll pick it up.

This avoids message loss when tmux injection is temporarily unavailable.

---

## 9. Relationship to tmux

tmux is used in two places:

- **Injection**: to nudge an active agent to process its inbox or to surface a new message
- **Observation**: to inspect an agent's pane for response context if the controller needs to correlate behavior

tmux is not used as a direct peer-to-peer transport for agent messaging.

That distinction matters:
- tmux output is ephemeral,
- mailbox files are durable,
- and the durable store is what makes the messaging path reliable.

---

## 10. Storage Layout

The message inbox stays under the existing channel tree:

```text
.aegis/channels/<agent_id>/inbox/
  <timestamp>_<message_id>.json
```

This keeps message delivery scoped to the current project and aligned with the rest of the filesystem channel design.

No new top-level storage root is required.

---

## 11. Security and Scope

Messaging is project-local.

The controller should ensure:
- the recipient belongs to the current project,
- the sender is authorized in the current project context,
- and the target ID resolves only inside the active project registry.

No host-wide discovery should leak agent identities between projects.

---

## 12. Testing Strategy

The first implementation pass should cover:

- recipient ID resolution by exact UUID and unique prefix,
- mailbox write durability and JSON shape,
- message ordering by timestamp and UUID,
- queueing to paused recipients,
- rejection of unknown or ambiguous recipient IDs,
- optional inbox prompt injection behavior,
- and cross-role delivery for Bastion-to-Bastion and Bastion-to-Splinter messages.

The main acceptance criteria is simple:
- one agent can address another agent by ID,
- the message lands in the target inbox,
- and the target can later read it without the sender needing any direct tmux linkage.

---

## 13. Open Questions

1. Should an active recipient always get a prompt nudge, or should nudging be opt-in?
2. Should archived terminal agents accept queued messages for later recovery?
3. Should the first CLI command be `aegis message` or a subcommand under `aegis channel`?
4. Should acknowledgements be persisted in the mailbox tree or in a separate task registry?

These are implementation choices and should be settled before the first code task begins.

