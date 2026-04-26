# LLD-B: Milestone Worktree Lifecycle

## Purpose

Defines the create/merge/cleanup lifecycle for per-milestone git worktrees. Builds on the existing `GitWorktree` helper in `crates/aegis-controller/src/git.rs`.

---

## Existing Infrastructure

`git.rs` already provides:
- `GitWorktree::create_for_agent(agent_id, role)` — creates worktree at `.aegis/worktrees/<uuid>/` on branch `aegis/<role>/<short_id>`.
- `GitWorktree::prune_for_agent(agent_id)` — removes a worktree.

For milestone worktrees the naming scheme changes: the identifier is the milestone ID string (e.g. `M29`), not an agent UUID. The lifecycle is longer than a single splinter's lifetime.

---

## New `git.rs` Methods

```rust
impl GitWorktree {
    /// Creates a worktree for a milestone at `.aegis/worktrees/milestone-<id>/`
    /// on branch `aegis/milestone/<id>`. Idempotent: if the worktree already
    /// exists returns its path without error.
    pub async fn create_for_milestone(&self, milestone_id: &str) -> Result<PathBuf>

    /// Merges `aegis/milestone/<id>` into `main` (fast-forward preferred,
    /// falls back to merge commit). On success removes the worktree and
    /// prunes the branch. Returns `Err(AegisError::GitMergeConflict)` if
    /// the merge produces conflicts.
    pub async fn merge_milestone_into_main(&self, milestone_id: &str) -> Result<()>

    /// Returns all active milestone worktree paths and their branch names.
    pub async fn list_milestone_worktrees(&self) -> Result<Vec<(String, PathBuf)>>
}
```

Branch naming: `aegis/milestone/<milestone_id>` (e.g. `aegis/milestone/M29`).  
Path: `.aegis/worktrees/milestone-<milestone_id>/` (e.g. `.aegis/worktrees/milestone-M29/`).

New error variants needed in `aegis-core`:
```
AegisError::GitMergeConflict { branch: String, reason: String }
```

---

## IPC Commands

Three new handlers in `crates/aegis-controller/src/daemon/uds.rs`:

| IPC command | Params | Response |
|-------------|--------|----------|
| `worktree.create` | `{"milestone_id": "M29"}` | `{"path": ".aegis/worktrees/milestone-M29", "branch": "aegis/milestone/M29"}` |
| `worktree.merge` | `{"milestone_id": "M29"}` | `{}` on success; error on conflict |
| `worktree.list` | `{}` | `[{"milestone_id": "M29", "path": "...", "branch": "..."}]` |

Handlers call the corresponding `Commands::worktree_*` async methods.

---

## CLI Subcommands

New `aegis worktree` top-level subcommand group in `src/main.rs`:

```
aegis worktree create <milestone-id>
aegis worktree merge  <milestone-id>
aegis worktree list
```

`src/commands/worktree.rs` — new file, mirrors `design.rs` style.

---

## Merge Sequence Detail

`merge_milestone_into_main`:
1. `git -C <project_root> checkout main`
2. `git -C <project_root> merge --no-ff aegis/milestone/<id> -m "chore: merge milestone <id>"`
3. On conflict: `git merge --abort`, return `AegisError::GitMergeConflict`.
4. On success:
   a. `git worktree remove --force .aegis/worktrees/milestone-<id>`
   b. `git branch -d aegis/milestone/<id>`
   c. `git worktree prune`

---

## Splinter Worktree Interaction

Splinter agents already get their own per-agent worktree (from existing spawn logic in `dispatcher.rs`). This does NOT change. The milestone worktree is the merge target that the bastion manages. Splinters commit to their per-agent branches; the bastion cherry-picks or merges those commits into the milestone branch before triggering the main merge.

Wait — this contradicts the Q1/Q6 answers (splinter commits its own work, all share one milestone worktree). Resolution:

**Revised approach**: For milestone worktrees, splinters are spawned with the milestone worktree as their working directory instead of their own per-agent worktree. The dispatcher's `spawn_from_template` for splinters spawned under a milestone receives the milestone worktree path via the `AgentSpec.worktree_override: Option<PathBuf>` field (new field). When set, `build_spawn_plan` uses it instead of creating a new per-agent worktree.

This keeps it per-milestone worktree, all splinters share one directory, each commits sequentially (the bastion assigns tasks and waits for each done before assigning the next within a window). "Parallel" means the bastion can have multiple splinters active simultaneously but they are expected to work on non-overlapping file sets.

New field on `AgentSpec`:
```rust
pub worktree_override: Option<PathBuf>,
```

---

## File Changes Summary

| File | Change |
|------|--------|
| `crates/aegis-controller/src/git.rs` | Add `create_for_milestone`, `merge_milestone_into_main`, `list_milestone_worktrees` |
| `crates/aegis-core/src/error.rs` | Add `AegisError::GitMergeConflict` |
| `crates/aegis-controller/src/lifecycle.rs` | Add `worktree_override: Option<PathBuf>` to `AgentSpec` |
| `crates/aegis-controller/src/dispatcher.rs` | Use `worktree_override` in `build_spawn_plan` when set |
| `crates/aegis-controller/src/commands.rs` | Add `worktree_create`, `worktree_merge`, `worktree_list` |
| `crates/aegis-controller/src/daemon/uds.rs` | Add `worktree.create`, `worktree.merge`, `worktree.list` handlers |
| `src/commands/worktree.rs` | New file: `create`, `merge`, `list` CLI functions |
| `src/main.rs` | Add `Worktree` command group and dispatch |
