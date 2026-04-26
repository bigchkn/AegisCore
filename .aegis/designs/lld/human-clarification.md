# LLD: Human Clarification Loop

**Milestone:** M25  
**Status:** draft  
**HLD refs:** §1.1, §2.2, §4.5, §4.6, §5, §14  
**Builds on:** M23 Agent-to-Agent Messaging

---

## 1. Purpose

This document defines the path for an agent to ask the human a blocking question, wait durably for a reply, and resume work with the response attached to the original request.

The core requirement is:
- an agent can mark a task as blocked on clarification,
- the controller can surface that question to the human,
- the human can answer later through an approved interface,
- and the answer is delivered back to the requesting agent without losing context.

This is a controller-mediated loop. The agent does not speak directly to the human transport. It emits a structured clarification request, and the controller handles persistence, delivery, and reply correlation.

---

## 2. Non-Goals

This milestone does not introduce:
- free-form chat between agents and humans as a general conversational layer,
- host-wide message routing outside the current project,
- a new broker or external queue service,
- synchronous RPC-style blocking on the agent process itself,
- or a requirement that every human reply happen in a terminal.

The design is narrow: ask, queue, wait, answer, resume.

---

## 3. Design Summary

The clarification loop extends the M23 messaging model with a human recipient and a wait state:

1. An agent creates a clarification request when it needs missing information.
2. The controller stores the request durably and exposes it to the human inbox.
3. The agent enters a waiting state that preserves the task context.
4. The human replies through a controller-approved interface.
5. The controller correlates the reply with the request and delivers it back to the agent.
6. The agent resumes from the same task context with the answer attached.

The important property is durability:
- the request must survive daemon restarts,
- the answer must survive transport failure,
- and the pending state must not be lost if the agent pane closes.

---

## 4. Message Flow

### 4.1 Agent-side request flow

1. The agent determines it cannot continue safely without human input.
2. It emits a structured clarification request with:
   - `request_id`
   - `agent_id`
   - `task_id`
   - `question`
   - `context`
   - `priority`
   - `created_at`
3. The controller records the request and marks the agent as waiting.
4. The agent pauses on the clarification barrier instead of inventing an answer.

### 4.2 Human delivery flow

1. The controller exposes pending requests to the human through the approved surface.
2. The human sees enough context to answer the question without needing to inspect raw agent state.
3. The human submits a response that includes:
   - the `request_id`
   - the answer text or structured payload
   - optional routing hints such as `apply_to_task` or `defer`
4. The controller persists the response and marks the request answered.

### 4.3 Agent resume flow

1. The controller writes the response back to the requesting agent inbox.
2. If the agent is active, it can be nudged to continue.
3. If the agent is paused or offline, the answer remains queued.
4. When the agent resumes, it reads the clarification response and continues the task.

---

## 5. Data Model

Clarification requests should be modeled as a durable record separate from transient console output.

```rust
pub struct ClarificationRequest {
    pub request_id: Uuid,
    pub agent_id: Uuid,
    pub task_id: Option<Uuid>,
    pub question: String,
    pub context: serde_json::Value,
    pub priority: i32,
    pub status: ClarificationStatus,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
}

pub enum ClarificationStatus {
    Open,
    Answered,
    Rejected,
    Expired,
}

pub struct ClarificationResponse {
    pub request_id: Uuid,
    pub answer: String,
    pub payload: serde_json::Value,
    pub answered_by: ClarifierSource,
    pub created_at: DateTime<Utc>,
}
```

The request record is the source of truth. The inbox message is the transport copy used to deliver the answer back to the agent.

---

## 6. Storage

The clarification loop needs a durable project-local queue. The recommended storage layout is:

- `.aegis/state/clarifications.json` for request metadata and status
- `.aegis/channels/human/inbox/` for pending human-facing items
- `.aegis/channels/<agent_id>/inbox/` for agent-facing replies
- `.aegis/handoff/<task_id>/` for optional task context snapshots when a question blocks progress

Requests should be append-safe and recoverable after daemon restart. If the human inbox is empty, the controller should rebuild the active queue from `clarifications.json`.

---

## 7. Routing Rules

### 7.1 Recipient model

The human is treated as a special recipient class, not as an agent ID.

Supported recipients are:
- a specific agent
- the project human

The human-facing queue belongs to the current project only. There is no host-wide clarification inbox.

### 7.2 Request correlation

Every answer must reference the originating `request_id`.

That allows:
- multiple outstanding questions from the same agent,
- multiple agents asking related questions at once,
- and deterministic replay after a daemon restart.

### 7.3 Ordering

Clarification requests are ordered by:
- `priority`
- `created_at`
- `request_id`

That lets a human answer urgent blockers first without losing older pending items.

---

## 8. Human Interfaces

The human can answer requests through one or more controller-approved surfaces.

Recommended surfaces:
- CLI
- TUI
- Telegram bridge

The transport should be an implementation detail. The clarification model must remain the same regardless of interface.

Recommended CLI shape:

```bash
aegis clarify list
aegis clarify show <request_id>
aegis clarify answer <request_id> "<response>"
aegis clarify wait <agent_id>
```

`wait` is useful when the operator wants to block until the next answer is available, but the daemon remains the owner of the durable state.

---

## 9. Waiting Semantics

The agent must not busy-loop while waiting.

The recommended behavior is:
- transition the agent to a `waiting` or `paused` clarification state,
- persist the request before the wait begins,
- and resume only when the correlated answer arrives or the request expires.

Timeout behavior should be explicit:
- no answer before timeout -> request becomes `Expired`
- expired requests should remain visible for audit
- the agent may either re-ask or fail the task, depending on policy

This avoids hidden stalls where the agent appears active but cannot make progress.

---

## 10. Controller API Surface

The controller should expose a clarification router rather than making every caller manage inbox files directly.

Recommended commands:
- `clarify.request`
- `clarify.list`
- `clarify.show`
- `clarify.answer`
- `clarify.wait`

These commands should integrate with the existing task, registry, and channel layers:
- task metadata identifies what is blocked,
- registry state identifies which agent is waiting,
- and the channel layer delivers the queued request and response.

---

## 11. Relationship to M23 Messaging

M23 provides the core message routing primitives.

This milestone reuses that foundation and adds:
- a human recipient class,
- a blocking/waiting state,
- and lifecycle rules for question/answer correlation.

The message transport should not be duplicated. The clarification loop should build on the existing mailbox and controller routing conventions rather than inventing a parallel transport stack.

---

## 12. Failure Handling

### 12.1 Request write failure

If the clarification request cannot be persisted:
- return an error to the agent,
- do not enter a waiting state,
- and do not surface the request as open.

### 12.2 Human answer delivery failure

If the answer cannot be delivered back to the agent:
- keep the response durably recorded,
- keep the request marked answered,
- and retry delivery later.

### 12.3 Daemon restart

On restart, the controller must reconstruct:
- open clarification requests,
- answered-but-undelivered responses,
- and waiting agent state.

This keeps the loop resilient even if the daemon restarts while a human is thinking.

---

## 13. Open Design Questions

These should be resolved before implementation:
- whether the waiting agent state should be `paused` or a dedicated `waiting` status,
- whether Telegram is mandatory for human answers or only one supported surface,
- whether clarification responses should support structured JSON payloads in addition to plain text,
- and whether unanswered requests should expire automatically or only by operator action.

---

## 14. Outcome

When this milestone is implemented, an agent will be able to say in effect:

> I am blocked because I need human clarification. Please ask the human, wait, and resume me with the answer.

That is the target behavior. The implementation tasks should now be able to split cleanly across:
- storage,
- controller routing,
- CLI/TUI surfaces,
- and agent resume behavior.
