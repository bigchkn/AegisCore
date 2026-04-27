# HLD: Web Task Editing & Bug Logging

**Milestone:** M34  
**Status:** draft  
**HLD ref:** this document  
**Scope:** `aegis-web`, `aegis-controller`, `aegis-taskflow`

## 1. Purpose

The web UI should let an operator create new tasks and edit existing tasks without dropping to the CLI. The primary use case is logging new bugs directly from the browser, but the same path should support task edits for existing roadmap items.

This milestone treats the web UI as a first-class taskflow editor:

- create a new task or bug
- edit existing task metadata
- persist the change through the controller
- automatically trigger `aegis taskflow notify` behavior after a successful mutation

The notify step is required so the running bastion can re-read the roadmap and react to the change without waiting for a manual refresh.

## 2. Current State

The web UI already renders taskflow status and milestone details. The controller already exposes taskflow read/write primitives through the CLI and daemon:

- `taskflow.status`
- `taskflow.show`
- `taskflow.add_task`
- `taskflow.set_task_status`
- `taskflow.notify`

What is missing is a web-facing editor surface and a mutation path that reliably updates the roadmap and then notifies active automation.

## 3. Scope

### In scope

- add-task flow from the web UI
- edit-task flow from the web UI
- bug logging from the web UI
- automatic taskflow notify after a successful save
- validation and failure feedback in the browser
- unit and integration tests around the new mutation path

### Out of scope

- task assignment to runtime agents
- task execution or agent lifecycle changes
- roadmap persistence format changes unless required by the editor API
- any new bastion logic beyond reacting to taskflow notify

## 4. User Flows

### 4.1 Create a new bug

1. Operator opens the taskflow view in the web UI.
2. Operator selects `Bug` as the task type.
3. Operator enters title, optional description, and target milestone or backlog.
4. Operator submits the form.
5. Controller writes the new task to taskflow storage.
6. Controller runs taskflow notify behavior.
7. Web UI refreshes the taskflow view.

### 4.2 Edit an existing task

1. Operator opens an existing task from the web UI.
2. Operator changes fields such as title, description, type, milestone, or status.
3. Operator saves the task.
4. Controller persists the edit.
5. Controller runs taskflow notify behavior.
6. Web UI refreshes affected taskflow views.

## 5. Proposed UI Shape

The initial web surface should live near the existing taskflow view:

- a create button for new tasks and bugs
- an edit action on each task row
- a modal editor for task field changes
- explicit task type selection with `Feature`, `Bug`, and `Maintenance`
- validation messages near the edited fields
- a save button in the top-right corner of the modal
- cancel controls with disabled state while the mutation is in flight

The editor should default to creating bugs in the backlog when the operator opens the “new bug” path, since that is the highest-frequency use case.

## 6. Backend Path

The web UI should not mutate roadmap files directly.

Preferred path:

1. Frontend submits a task mutation request to the controller.
2. Controller applies the taskflow write.
3. Controller invokes taskflow notify after the write succeeds.
4. Controller returns the updated task or milestone state.

This keeps the notify side effect close to the write and ensures it happens even if the browser does not explicitly make a second call.

### 6.1 Controller surface

The implementation can reuse the current command dispatch layer and add a small mutation API for the editor:

- create task
- update task
- optionally update task status as part of edit flow

The exact command names are left for the LLD, but the web path should stay aligned with the existing taskflow controller model instead of bypassing it.

## 7. Notify Behavior

The editor must trigger the existing `aegis taskflow notify` behavior after successful create/edit operations.

This should be treated as part of the mutation transaction from the user’s perspective:

- if the write fails, do not notify
- if the write succeeds, notify automatically
- if notify fails, surface that as a recoverable backend error so the operator knows the roadmap changed but the bastion may not have been nudged

That keeps automation responsive without requiring the operator to remember a second command.

## 8. Data Model

The editor needs enough task metadata to support both new tasks and edits:

- task title
- optional description or body
- task type
- target milestone or backlog
- task status

The web UI should preserve the current taskflow semantics:

- bugs can live in the backlog
- milestone tasks remain attached to their milestone
- edits should not silently move tasks between sections unless the operator explicitly chooses that target

## 9. Implementation Shape

The likely implementation split is:

- `aegis-web` frontend form and task edit panel
- `aegis-controller` mutation handlers and notify-after-write path
- `aegis-taskflow` helper methods if task mutation logic needs to be centralized

The existing taskflow read endpoints and CLI commands should remain usable. The web editor is an additional path, not a replacement.

## 10. Tests

The first tests should cover:

- creating a bug from the web editor writes the task and triggers notify
- editing an existing task writes the update and triggers notify
- validation failures do not trigger notify
- the UI opens the editor for an existing task
- the new-bug path defaults to a bug/backlog-oriented task state

## 11. Decisions

The design is now fixed to the following choices:

- task editing is available from the Taskflow view only
- the editor should expose all task fields in v1
- new bugs default to the backlog
- notify failures after a successful write should surface as a warning, not a hard failure
- the implementation should reuse the existing controller command path where feasible
