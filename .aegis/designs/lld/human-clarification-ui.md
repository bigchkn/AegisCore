# LLD: Human Clarification UI

**Milestone:** M26  
**Status:** draft  
**HLD refs:** §5, §14  
**Builds on:** M25 Human Clarification Loop

---

## 1. Purpose

This document defines the implementation of user interfaces for responding to human clarification requests in both the Aegis TUI and Web UI.

While M25 implemented the backend logic for queuing and correlates requests/answers, this milestone (M26) provides the actual human-facing tools to perform the "Answer" step.

---

## 2. TUI Implementation: Agent Overlay

In the TUI, clarification requests are surfaced when a user inspects an agent that has an `AgentStatus::Paused` status due to a pending clarification.

### 2.1 UX Flow
1. User selects an agent in the TUI list.
2. If the agent is waiting on a clarification, a notification indicator appears.
3. User presses `r` (for "Reply").
4. A centered modal/overlay appears, showing the question text and a textarea for the response.
5. User types the answer and presses `Enter` to submit.
6. User presses `Esc` to cancel.
7. Upon submission, the TUI sends a `clarify.answer` UDS command.

### 2.2 Component Design
- `ReplyModal`: A new widget in `crates/aegis-tui/src/ui.rs` that takes the `ClarificationRequest` and a mutable input buffer.
- `Handler`: `handler.rs` must handle character input when the modal is active.

---

## 3. Web UI Implementation: Dedicated View

In the Web UI, a dedicated "Clarifications" view is provided to allow for cross-agent triage of pending requests.

### 3.1 UX Flow
1. User selects "Clarifications" from the main Sidebar.
2. The view displays a list of all `ClarificationStatus::Open` requests for the active project.
3. Each request is rendered as a card showing:
   - Request ID
   - Agent ID / Name
   - Question text
   - Timestamp
4. A "Reply" button or an inline textarea allows the user to provide an answer.
5. Clicking "Submit" triggers the `clarify.answer` REST API call.

### 3.2 Component Design
- `ClarificationsView.tsx`: Main view component that polls `clarify.list`.
- `ClarificationCard`: A sub-component rendering a single request and its reply form.

---

## 4. API & Models

The UI will use the existing `ClarificationRequest` and `ClarificationStatus` models.

### 4.1 Web REST API
- `GET /projects/:pid/clarify/list`: Returns `Vec<ClarificationRequest>`.
- `POST /projects/:pid/clarify/answer`: Accepts `{ "request_id": Uuid, "answer": String, "payload": Value }`.

### 4.2 TUI UDS API
- `clarify.list` (json-rpc): List pending requests.
- `clarify.answer` (json-rpc): Submit an answer.

---

## 5. Milestone Tasks

- [ ] (Web) Define `ClarificationRequest` and related types in `domain.ts`.
- [ ] (Web) Implement `clarify.list` and `clarify.answer` bindings in `rest.ts`.
- [ ] (Web) Implement `ClarificationsView.tsx` with polling and reply logic.
- [ ] (Web) Add routing for `/clarifications` in `App.tsx` and sidebar.
- [ ] (TUI) Expand UDS client with `clarify_list` and `clarify_answer`.
- [ ] (TUI) Add `active_clarification` and input buffer to `App` state.
- [ ] (TUI) Implement the `ReplyModal` widget in `ui.rs`.
- [ ] (TUI) Update `handler.rs` to manage modal keybinds and text input.
