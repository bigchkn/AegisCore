use aegis_core::provider::FailoverContext;

pub fn render_handoff_prompt(ctx: &FailoverContext) -> String {
    format!(
        "You are resuming work from a previous agent ({previous}) that stopped unexpectedly.\n\
         \n\
         Your working directory: {worktree}\n\
         Your role: {role}\n\
         {task_section}\
         \n\
         Below is the terminal output from the previous agent (last {lines} lines). \
         Review it to understand what was completed and what remains:\n\
         \n\
         ---\n\
         {context}\n\
         ---\n\
         \n\
         Resume the task from where the previous agent left off. \
         Do not restart from scratch. Write [AEGIS:DONE] when complete.",
        previous = ctx.previous_provider,
        worktree = ctx.worktree_path.display(),
        role = ctx.role,
        task_section = ctx
            .task_description
            .as_deref()
            .map(|t| format!("Task: {t}\n"))
            .unwrap_or_default(),
        lines = ctx.terminal_context.lines().count(),
        context = ctx.terminal_context,
    )
}
