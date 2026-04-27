Your task: **{{task_description}}**
LLD reference: `{{lld_path}}`
Coordinator ID: `{{bastion_agent_id}}`

---

**Step 1** — Check your inbox for context from the coordinator:

```
aegis message inbox
```

**Step 2** — Read the LLD at `{{lld_path}}`. Implement the task as described. Stay within your assigned worktree.

**Step 3** — Run the relevant tests. Check the LLD for which crate to target:

```
cargo test -p <crate>
```

**Step 4** — When all tests pass, commit your changes:

```
git add <changed files>
git commit -m "<concise description of what changed>"
```

**Step 5** — Notify your coordinator and stop:

```
aegis message send {{bastion_agent_id}} notification \
  '{"status":"done","task_id":"{{task_id}}","summary":"<one-line description>"}'
aegis agent exit self
```

If at any point you are blocked (missing context, failing tests you cannot fix, ambiguous requirements), commit any partial work, then send a blocked notification and stop:

```
aegis message send {{bastion_agent_id}} notification \
  '{"status":"blocked","task_id":"{{task_id}}","reason":"<explanation>"}'
```
