# LLD: Taskflow Engine (`aegis-taskflow`)

**Milestone:** M13  
**Status:** draft  
**HLD ref:** §15, §16.3  
**Implements:** `crates/aegis-taskflow/`

---

## 1. Purpose

The Taskflow Engine manages the high-level project development lifecycle by bridging documentation (HLD, LLD, Roadmap) with execution state (Agent Tasks). It uses a **Modular Content** model to ensure the "Project Brain" remains merge-friendly and context-efficient as the project grows.

AegisCore projects are "self-documenting" via the `.aegis/designs/` directory. Taskflow is the orchestrator that aggregates fragments into a coherent project view.

---

## 2. Module Structure

```
crates/aegis-taskflow/
├── Cargo.toml
└── src/
    ├── lib.rs          ← Public API and Engine struct
    ├── parser/
    │   ├── mod.rs      ← Entry point for TOML and Markdown fragments
    │   ├── milestone.rs ← TOML Milestone parser
    │   ├── index.rs     ← Project Index parser
    │   └── design.rs    ← LLD/HLD Markdown parser
    ├── model.rs        ← TaskflowStatus, Milestone, ProjectTask types
    ├── state.rs        ← TaskflowLinkRegistry (roadmap ID → Registry UUID)
    └── sync.rs         ← Synchronization logic
```

---

## 3. Data Model (Modular)

### 3.1 Project Index (`roadmap/index.toml`)

The entry point for all project discovery.

```toml
[project]
name = "AegisCore"
current_milestone = 13

[milestones]
M11 = { path = "milestones/M11.toml", status = "done" }
M13 = { path = "milestones/M13.toml", status = "in-progress" }
```

### 3.2 Milestone Fragment (`roadmap/milestones/M<N>.toml`)

One file per milestone to prevent Git merge conflicts.

```toml
id = 13
name = "Taskflow Engine"
status = "in-progress"
lld = "lld/taskflow.md" # Link to design doc

[[tasks]]
id = "13.1"
task = "Write lld/taskflow.md"
status = "done"

[[tasks]]
id = "13.2"
task = "Implement modular TOML parser"
status = "in-progress"
```

---

## 4. Agent Discovery & Access (CLI-First)

The primary method for agents to interact with the Taskflow Engine is via the `aegis` CLI proxy. This ensures that agents receive token-optimized, pre-parsed summaries rather than raw TOML/Markdown.

### 4.1 CLI Interface Summary

- `aegis taskflow status`: Returns a high-level summary of the project pipeline (active milestones, overall % complete).
- `aegis taskflow list`: Returns a list of all milestones, their IDs, and current statuses.
- `aegis taskflow show <M-ID>`: Returns the detailed task list and design links for a specific milestone.
- `aegis taskflow assign <roadmap_id> <task_id>`: (Architects only) Link a roadmap task to an execution task.

### 4.2 System Prompt Snippet

Every agent spawned in an Aegis project is provided with the following instruction snippet in their system prompt to ensure "Taskflow Awareness":

```markdown
### Project Context & Navigation
You are operating within an AegisCore autonomous environment. To understand your place in the broader project roadmap, use the following tools:
- Run `aegis taskflow status` to see the overall project health.
- Run `aegis taskflow show <M-ID>` (e.g., M13) to see the specific tasks and design goals for your current milestone.
- Read design documents directly at `.aegis/designs/` for deep technical context (Read-Only).
```

---

## 5. Synchronization Logic

The `TaskflowEngine` provides a `sync()` method:

1.  **Aggregate Fragments**: Read `index.toml` and all referenced milestone TOMLs.
2.  **Cross-Reference Registry**: Get execution tasks from `registry.json`.
3.  **Update Statuses**:
    - If the linked Registry Task is `Complete` → Update TOML task to `done`.
    - If the linked Registry Task is `Active` → Update TOML task to `in-progress`.
4.  **Auto-Generate View**: (Optional) Generate a project-wide `roadmap.md` for human consumption based on the modular fragments.

---

## 6. Security & Permissions

- **Splinter Agents**: Denied `file-write*` on `.aegis/designs/`. They can only read the roadmap.
- **Architect Bastions**: Granted `file-write*` on `.aegis/designs/`. They are the only agents allowed to evolve the roadmap and milestones.

---

## 7. Test Strategy

| Test | Asserts |
|---|---|
| `test_modular_parsing` | Correctly aggregates index and multiple milestone fragments |
| `test_lazy_loading` | Engine can load a single milestone without reading the entire project |
| `test_sync_updates_toml` | `sync()` correctly modifies the status in the modular TOML files |
| `test_merge_safety` | Concurrent writes to different milestone files do not conflict |
