# LLD-C: Milestone Dependency Ordering & `aegis taskflow next`

## Purpose

Defines the `depends_on` data model addition to milestones, the topological sort algorithm the bastion uses to pick the next milestone, and the `aegis taskflow next` CLI command.

---

## Data Model Change

Add optional field to `Milestone` in `crates/aegis-taskflow/src/model.rs`:

```rust
pub struct Milestone {
    pub id: u32,
    pub name: String,
    pub status: String,
    pub lld: Option<String>,
    pub tasks: Vec<ProjectTask>,
    #[serde(default)]
    pub depends_on: Vec<String>,   // ← new; milestone IDs as strings e.g. ["M27", "M28"]
}
```

The field is `#[serde(default)]` so all existing milestone TOMLs remain valid (empty vec means no dependencies).

Example in a milestone TOML:
```toml
id = 30
depends_on = ["M28"]
```

---

## Algorithm: Greedy Topological Next

`aegis taskflow next` returns the **single** next milestone the bastion should work on.

```
1. Load all milestones from the roadmap index.
2. Filter to status == "pending".
3. For each pending milestone, check if ALL milestones in depends_on have status == "done".
   - A milestone is "ready" if depends_on is empty OR all deps are done.
4. From the ready set, return the one with the lowest numeric ID.
5. If the ready set is empty but there are pending milestones with unresolved deps:
   - Return an error "blocked: waiting on dependencies" with details.
6. If there are no pending milestones at all:
   - Return an error "exhausted: no pending milestones".
```

The bastion uses the exit code and/or JSON output to distinguish EXHAUSTED (sleep + await notification) from BLOCKED (clarify with human).

---

## IPC & CLI

New IPC command `taskflow.next`:
- Params: `{}` (no parameters)
- Response (success): `{"milestone_id": "M30", "name": "...", "task_count": 5}`
- Response (exhausted): HTTP 404 / `{"error": "exhausted"}`
- Response (blocked): `{"error": "blocked", "waiting_on": ["M28"]}`

New CLI subcommand:
```
aegis taskflow next [--json]
```

Human-readable output:
```
Next milestone: M30 — Milestone Worktree Lifecycle (5 tasks)
```
or:
```
No pending milestones — backlog is exhausted.
```

---

## File Changes Summary

| File | Change |
|------|--------|
| `crates/aegis-taskflow/src/model.rs` | Add `depends_on: Vec<String>` with `#[serde(default)]` to `Milestone` |
| `crates/aegis-taskflow/src/lib.rs` | Add `next_milestone(roadmap: &Roadmap) -> Result<Option<MilestoneRef>>` |
| `crates/aegis-controller/src/daemon/uds.rs` | Add `taskflow.next` IPC handler |
| `crates/aegis-controller/src/commands.rs` | Add `taskflow_next()` async method |
| `src/commands/taskflow.rs` | Add `next()` function |
| `src/main.rs` | Add `Next` variant to `TaskflowCommands` |

---

## Test Cases

- `next_returns_lowest_ready_milestone` — two pending milestones, no deps, returns lower ID
- `next_skips_milestone_with_unmet_dep` — M31 depends on M30 which is pending, returns M30
- `next_returns_exhausted_when_all_done` — all milestones done, returns exhausted
- `next_returns_blocked_when_only_unmet_deps_remain` — only M31 remains, dep M30 is pending (not done), returns blocked
