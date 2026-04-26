# LLD: Global Backlog & Task Types

## Overview
This document outlines the design for decoupling tasks from strict milestone requirements by introducing a global "Backlog" and categorizing tasks by type (Feature, Bug, Maintenance).

## Data Model Changes

### 1. `TaskType` Enum
Added to `crates/aegis-taskflow/src/model.rs`:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    Feature,
    Bug,
    Maintenance,
}
```

### 2. `ProjectTask` Update
Add `task_type` field:
```rust
pub struct ProjectTask {
    pub uid: Uuid,
    pub id: String,
    pub task: String,
    pub task_type: TaskType, // New field
    pub status: TaskStatus,
    pub priority: u8,
    pub registry_task_id: Option<String>,
}
```

## Storage Strategy

### Global Backlog (`backlog.toml`)
A new file located at `.aegis/designs/roadmap/backlog.toml`. 
Structure:
```toml
[backlog]
tasks = [
    { id = "B1", task = "Fix race condition in LockedFile", task_type = "Bug", status = "pending", priority = 1 },
]
```

## Engine Integration

### `TaskflowEngine`
- Modify `load_project` to also read `backlog.toml` if it exists.
- The `ProjectState` struct will now hold a `backlog: Vec<ProjectTask>` field.
- `sync` and `status` commands will include backlog tasks.

## CLI Enhancements

### `aegis taskflow add-task`
Add optional flags:
- `--bug`: Sets `task_type` to `Bug` and defaults target to backlog if no milestone is provided.
- `--maint`: Sets `task_type` to `Maintenance`.

Example:
```bash
aegis taskflow add-task --bug "Connection timeout on UDS"
```

### `aegis taskflow show backlog`
A special ID "backlog" will be reserved in the CLI/Daemon to show the global backlog instead of a milestone.
