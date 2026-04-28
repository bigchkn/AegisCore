use aegis_core::provider::FailoverContext;

pub fn render_handoff_prompt(ctx: &FailoverContext) -> String {
    let compressed_context = compress_terminal_context(&ctx.terminal_context);
    format!(
        "You are resuming work from a previous agent ({previous}) that stopped unexpectedly.\n\
         \n\
         Your working directory: {worktree}\n\
         Your role: {role}\n\
         {task_section}\
         \n\
         Original Instructions:\n\
         {system_prompt}\n\
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
        system_prompt = ctx.system_prompt,
        lines = ctx.terminal_context.lines().count(),
        context = compressed_context,
    )
}

fn compress_terminal_context(context: &str) -> String {
    let mut result = String::with_capacity(context.len());
    let mut in_spaces = false;

    for c in context.chars() {
        if c == ' ' {
            if !in_spaces {
                result.push(' ');
                in_spaces = true;
            }
        } else {
            result.push(c);
            in_spaces = false;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_terminal_context() {
        let input = "hello     world  test";
        assert_eq!(compress_terminal_context(input), "hello world test");
    }
}
