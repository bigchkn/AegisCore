# Taskflow System Guide

The **Taskflow Engine** is the orchestration heart of AegisCore. It bridges project documentation (HLD, LLD, Roadmap) with real-time execution state (Agent Tasks), ensuring that autonomous agents remain aligned with your project's high-level goals.

## 1. Core Philosophy

AegisCore projects are **self-documenting**. Instead of a single monolithic roadmap, Taskflow uses a **Modular Content** model:
- **Project Index**: A central manifest (`roadmap/index.toml`) tracking milestones.
- **Milestone Fragments**: Individual TOML files for each milestone (e.g., `M13.toml`), preventing Git merge conflicts.
- **Design Docs**: Markdown files in `.aegis/designs/lld/` linked directly to roadmap tasks.

### Collision Prevention & GUIDs
Every task in the roadmap is assigned a unique `uid` (UUID) upon creation. While tasks have friendly IDs (like `14.1`), the `uid` is the immutable internal identifier. This ensures that even if multiple agents attempt to modify the roadmap simultaneously, their work remains distinct and traceable.

### Concurrency Control
The Taskflow Engine employs **mandatory file locking** (`fs2`) and **atomic writes**. When an agent (or the CLI) modifies a roadmap fragment, it acquires an exclusive lock, ensuring that no two processes can overwrite each other's changes.

---

## 2. Roadmap Management via CLI

You can manage your project roadmap directly through the `aegis` CLI. This allows both humans and "Architect" agents to evolve the project structure without manual TOML editing.

### Creating a Milestone
```bash
aegis taskflow create-milestone 14 "Web UI Implementation" --lld "lld/web-ui.md"
```
This creates `milestones/M14.toml` and registers it in `index.toml`.

### Adding a Task to a Milestone
```bash
aegis taskflow add-task 14 14.1 "Setup React frontend project"
```

---

## 3. End-to-End Example: Implementing a Feature

Here is how a task moves from "Intention" to "Verified Completion":

### Step 1: Define the Milestone and Task
```bash
aegis taskflow create-milestone 15 "Security Audit"
aegis taskflow add-task 15 15.1 "Review sandbox profiles"
```

### Step 2: User spawns an execution task
The user spawns a Splinter agent to actually do the work:

```bash
aegis spawn "Review the .sb templates in crates/aegis-sandbox/templates"
# Output: Task enqueued with ID: 550e8400-e29b-41d4-a716-446655440000
```

### Step 3: Link Roadmap to Execution
Tell Aegis that roadmap task `15.1` is being handled by registry task `550e8400...`:

```bash
aegis taskflow assign 15.1 550e8400-e29b-41d4-a716-446655440000
```

### Step 4: Agent works and completes
The agent performs the work. Once the agent finishes and the registry marks the task as `Complete`:

### Step 5: Sync the Roadmap
Run the sync command to update the TOML files based on registry state:

```bash
aegis taskflow sync
# Output: Taskflow synced. 1 tasks updated.
```

If you check the milestone now, task `15.1` will automatically have its status updated to `done`.

---

## 4. CLI Command Reference

| Command | Description |
|---------|-------------|
| `aegis taskflow status` | High-level summary of active milestones and project health. |
| `aegis taskflow list` | Lists all milestones and their current statuses. |
| `aegis taskflow show <M-ID>` | Shows detailed task list and design links for a milestone (e.g., `M13`). |
| `aegis taskflow create-milestone <ID> <NAME>` | Create a new milestone fragment and register it. |
| `aegis taskflow add-task <M-ID> <ID> <TASK>` | Add a new task to an existing milestone. |
| `aegis taskflow set-task-status <M-ID> <TASK-ID> <STATUS>` | Update an existing roadmap task status. |
| `aegis taskflow sync` | Synchronizes the roadmap TOMLs with the actual Agent Registry state. |
| `aegis taskflow assign <roadmap_id> <task_id>` | Manually link a roadmap task to a specific agent task UUID. |

---

## 5. Agent Perspective

When an agent is spawned, it is instructed to use Taskflow to gain context. A typical agent workflow looks like this:

1. **Orientation**: Run `aegis taskflow status` to see where the project stands.
2. **Detail**: Run `aegis taskflow show M13` to see the current tasks.
3. **Execution**: Perform the task, then use `aegis taskflow sync` to update the roadmap once the registry reflects completion.
