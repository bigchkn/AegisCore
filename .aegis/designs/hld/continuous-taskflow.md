# HLD: Continuous Bastion Milestone-Processing Workflow

## Overview

The goal is a self-driving execution loop: a bastion agent starts, picks up the next pending milestone, delegates each task to splinter agents operating in an isolated worktree, commits each completed task, merges the milestone branch back into main, then moves on to the next milestone. The loop continues until the backlog is empty. External additions (new milestones or tasks) notify the running bastion so it can incorporate them without requiring a restart.

---

## Design Questions

These questions must be resolved before the LLD and implementation milestones are written. Please answer each one.

---

**Q1: Worktree granularity — per-milestone or per-task?**

Option A: One git worktree is created per milestone. All splinters for that milestone share the same worktree directory and commit directly to it sequentially.

Option B: One git worktree is created per task. Each splinter gets its own isolated branch, and the bastion squash-merges task branches into the milestone branch as each finishes.

Option A is simpler and the natural unit of isolation matches the merge boundary. Option B enables parallel splinters but introduces intra-milestone merge complexity. Which do you prefer, and is within-milestone task parallelism a requirement?

---

**Q2: Merge strategy — automatic or gated?**

When all tasks in a milestone are done, should the bastion:

Option A: Automatically run `git merge` (or `git rebase`) to land the milestone branch into `main` with no human checkpoint.

Option B: Open a PR / pause and notify the human via `aegis clarify` for review before merge.

Option C: Run tests first, then auto-merge only if they pass; otherwise pause and clarify.

Is there a CI gate requirement, or is this a trusted-agent environment where auto-merge is acceptable?

---

**Q3: Splinter concurrency within a milestone**

Should the bastion spawn splinters for tasks sequentially (one active at a time, next starts after previous reports done) or in parallel (multiple splinters working different tasks simultaneously)?

Sequential is required if tasks have intra-milestone dependencies. Parallel requires the per-task worktree model from Q1-B. Which mode is expected, or should this be determined per-milestone from task metadata?

---

**Q4: Bastion loop driver — polling or event-pushed?**

How should the bastion know the current state of the backlog?

Option A: The bastion polls `aegis taskflow status` on a timer (e.g., every 30 seconds) to check for new work.

Option B: The daemon pushes a message to the bastion via the M23 messaging system when the roadmap changes (new milestone added, task status changed from outside).

Option B is more responsive but requires a new daemon capability (roadmap file watch → IPC message). Is push notification required, or is polling acceptable?

---

**Q5: Roadmap change detection — what counts as "added outside the agent"?**

The bastion should react when someone adds a milestone or task while it is running. What triggers this?

Option A: A file-system watch on `.aegis/designs/roadmap/milestones/` — any write to a `.toml` file triggers the notification.

Option B: A new CLI command `aegis taskflow notify` that the human explicitly runs after editing the roadmap.

Option C: The next polling cycle picks it up automatically (no real-time notification needed).

How urgent is real-time notification? Would a ~30-second polling lag be acceptable, or does the bastion need to react within seconds?

---

**Q6: Commit ownership — splinter commits or bastion commits?**

When a splinter finishes a task:

Option A: The splinter commits its own work to the milestone worktree, then messages the bastion with `{"status":"done"}`. The bastion only observes.

Option B: The splinter messages the bastion with a diff or "ready to commit" signal; the bastion runs `git commit` itself after reviewing.

Option A is simpler and consistent with the current M22 protocol. Option B gives the bastion more control over commit messages and history shape. Which model do you want?

---

**Q7: Failure and retry policy**

When a splinter reports `{"status":"blocked"}`:

Option A: Bastion spawns a fresh splinter and retries the same task (up to N attempts).

Option B: Bastion sends a `clarify` request to the human and waits for direction before retrying.

Option C: Bastion marks the task `blocked` in taskflow and skips to the next task (human resolves later).

Should the retry count and escalation threshold be configurable per-milestone, or a global default? What is the default behavior?

---

**Q8: Bastion resume after crash or restart**

If the bastion process dies or the daemon restarts mid-milestone:

Option A: A new bastion reads taskflow status, finds the `in-progress` milestone and any `in-progress` tasks, re-spawns splinters only for incomplete tasks, and continues from there.

Option B: The new bastion always restarts the entire current milestone from scratch (simpler but potentially repeats work).

Option A requires the bastion to reason about partial worktree state. Does the bastion need to handle this autonomously, or is a human expected to reset and re-trigger after a crash?

---

**Q9: Milestone ordering and dependency**

How should the bastion decide which pending milestone to tackle next?

Option A: Numeric order (lowest milestone ID first).

Option B: A `priority` field in the milestone TOML.

Option C: A `depends_on = [...]` field in the milestone TOML; the bastion resolves the topological order.

Is there a need to express inter-milestone dependencies in the data model, or is strict numeric ordering sufficient for now?

---

**Q10: Human oversight gates**

At what points, if any, should the bastion pause and require explicit human approval before proceeding?

- Before starting a new milestone
- Before spawning the first splinter for a milestone
- Before the merge into main
- Never — fully autonomous until empty or blocked

Is the intent "as autonomous as possible with clarification only on failure," or should there be a configurable oversight level (e.g., `oversight = "none" | "pre-merge" | "pre-milestone"`)?

---

## Preliminary Scope Assessment

Based on the description, implementing this workflow requires changes in several areas. This is a rough pre-answer estimate — final scope will be determined once the questions above are answered.

### What already exists
- Bastion/splinter templates and `aegis design spawn` (M20–M22)
- Agent-to-agent messaging (M23)
- Human clarification loop (M25)
- Git worktree plumbing in the controller (M27)
- Taskflow status/sync CLI (M13)

### Net-new work anticipated

| Area | Description |
|------|-------------|
| Bastion loop template | Rewrite `taskflow-bastion` system prompt and startup to describe the full multi-milestone control loop, not a single milestone |
| Worktree lifecycle IPC | New commands: `worktree.create_for_milestone`, `worktree.merge_and_close` (or expose via CLI wrapper) |
| Roadmap file watcher | Daemon component that watches roadmap TOML files and sends a message to the active bastion agent on change |
| Bastion registry | Daemon tracks which agent holds the `bastion` role so the watcher knows where to route notifications |
| Splinter commit protocol | Define the commit-message format and `git add` scope a splinter is expected to use before reporting done |
| Resume protocol | Spec for how a fresh bastion reads partial taskflow state and reconstructs in-flight work |
| Milestone ordering | `aegis taskflow next` or equivalent query the bastion uses to pick the next milestone |

---

## Next Steps

1. User reviews this HLD and answers Q1–Q10
2. Write LLD documents for each implementation area
3. Create implementation milestones (M30+) derived from the answers

