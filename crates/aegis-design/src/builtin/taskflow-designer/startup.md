Your assignment: write a **{{doc_type}}** at `{{doc_path}}`.
Description: {{doc_description}}
Coordinator ID: `{{bastion_agent_id}}`

---

**Step 1** — Check your inbox for context from the coordinator:

```
aegis message inbox
```

**Step 2** — Orient yourself. Read the existing documents that provide context:

- If writing an LLD, read the parent HLD at `{{hld_ref}}` (if provided).
- Scan `.aegis/designs/lld/` (for LLD) or `.aegis/designs/hld/` (for HLD) for neighbouring documents to match style.
- Read relevant source files in `crates/` to ground claims about current behaviour.

**Step 3** — Write the document to `{{doc_path}}`. Use the canonical structure for your document
type (HLD or LLD) as defined in your system prompt. Be specific and actionable — an implementer
should be able to act on an LLD without asking clarifying questions.

**Step 4** — Notify your coordinator and stop:

```
aegis message send {{bastion_agent_id}} notification \
  '{"status":"done","task_id":"{{task_id}}","doc_path":"{{doc_path}}","summary":"<one-line description>"}'
```

If at any point you are blocked (missing critical context you cannot infer from the codebase):

```
aegis message send {{bastion_agent_id}} notification \
  '{"status":"blocked","task_id":"{{task_id}}","reason":"<explanation>"}'
```
