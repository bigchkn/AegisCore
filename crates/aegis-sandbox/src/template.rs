use std::path::Path;

use aegis_core::{SandboxNetworkPolicy, SandboxPolicy};

use crate::SandboxError;

pub const AGENT_JAIL_TEMPLATE: &str = include_str!("../templates/agent_jail.sb");
const NODE_MODULES_PATH: &str = "/opt/homebrew/lib/node_modules";

pub fn render_template(
    template: &str,
    worktree: &Path,
    home: &Path,
    aegis_logs_dir: &Path,
    policy: &SandboxPolicy,
) -> Result<String, SandboxError> {
    let worktree_path = path_to_sbpl(worktree)?;
    let home_path = path_to_sbpl(home)?;
    let logs_dir = path_to_sbpl(aegis_logs_dir)?;
    let extra_reads = render_allow_paths("file-read*", &policy.extra_reads)?;
    let extra_writes = render_allow_paths("file-write*", &policy.extra_writes)?;
    let extra_exec_paths = render_allow_paths("process-exec", &policy.extra_exec_paths)?;
    let hard_deny_reads = render_deny_paths("file-read*", &policy.hard_deny_reads)?;
    let network_policy = render_network_policy(&policy.network);

    let rendered = template
        .replace("@@WORKTREE_PATH@@", &worktree_path)
        .replace("@@HOME@@", &home_path)
        .replace("@@AEGIS_LOGS_DIR@@", &logs_dir)
        .replace("@@NODE_MODULES_PATH@@", NODE_MODULES_PATH)
        .replace("@@EXTRA_READS@@", &extra_reads)
        .replace("@@EXTRA_WRITES@@", &extra_writes)
        .replace("@@EXTRA_EXEC_PATHS@@", &extra_exec_paths)
        .replace("@@HARD_DENY_READS@@", &hard_deny_reads)
        .replace("@@NETWORK_POLICY@@", network_policy);

    if let Some(var) = first_unresolved_var(&rendered) {
        return Err(SandboxError::TemplateVar { var });
    }

    Ok(rendered)
}

fn render_allow_paths(
    operation: &str,
    paths: &[std::path::PathBuf],
) -> Result<String, SandboxError> {
    paths
        .iter()
        .map(|path| {
            let path = path_to_sbpl(path)?;
            Ok(format!("(allow {operation}\n  (subpath \"{path}\"))"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|blocks| blocks.join("\n"))
}

fn render_deny_paths(
    operation: &str,
    paths: &[std::path::PathBuf],
) -> Result<String, SandboxError> {
    paths
        .iter()
        .map(|path| {
            let path = path_to_sbpl(path)?;
            Ok(format!("(deny {operation}\n  (subpath \"{path}\"))"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|blocks| blocks.join("\n"))
}

fn render_network_policy(policy: &SandboxNetworkPolicy) -> &'static str {
    match policy {
        SandboxNetworkPolicy::None => "(deny network*)",
        SandboxNetworkPolicy::OutboundOnly => "(allow network-outbound)\n(deny network-inbound)",
        SandboxNetworkPolicy::Any => "(allow network*)",
    }
}

fn path_to_sbpl(path: &Path) -> Result<String, SandboxError> {
    let path = path.to_str().ok_or_else(|| SandboxError::NonUtf8Path {
        path: path.to_path_buf(),
    })?;
    Ok(escape_sbpl_string(path))
}

fn escape_sbpl_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn first_unresolved_var(rendered: &str) -> Option<String> {
    let start = rendered.find("@@")?;
    let rest = &rendered[start + 2..];
    let end = rest.find("@@")?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{SandboxNetworkPolicy, SandboxPolicy};
    use std::path::{Path, PathBuf};

    #[test]
    fn renders_network_variants() {
        assert_eq!(
            render_network_policy(&SandboxNetworkPolicy::None),
            "(deny network*)"
        );
        assert_eq!(
            render_network_policy(&SandboxNetworkPolicy::OutboundOnly),
            "(allow network-outbound)\n(deny network-inbound)"
        );
        assert_eq!(
            render_network_policy(&SandboxNetworkPolicy::Any),
            "(allow network*)"
        );
    }

    #[test]
    fn renders_configured_path_blocks() {
        let paths = vec![PathBuf::from("/usr/local/share/zsh")];
        let rendered = render_allow_paths("file-read*", &paths).expect("paths render");
        assert_eq!(
            rendered,
            "(allow file-read*\n  (subpath \"/usr/local/share/zsh\"))"
        );
    }

    #[test]
    fn rejects_unresolved_template_vars() {
        let err = render_template(
            "@@MISSING@@",
            Path::new("/tmp/wt"),
            Path::new("/Users/test"),
            Path::new("/tmp/wt/.aegis/logs/sessions"),
            &SandboxPolicy::default(),
        )
        .expect_err("missing token should fail");

        assert!(matches!(err, SandboxError::TemplateVar { var } if var == "MISSING"));
    }

    #[test]
    fn agent_jail_template_has_no_unresolved_placeholders() {
        let policy = SandboxPolicy {
            network: SandboxNetworkPolicy::OutboundOnly,
            extra_reads: vec![PathBuf::from("/extra/read")],
            extra_writes: vec![PathBuf::from("/extra/write")],
            extra_exec_paths: vec![PathBuf::from("/extra/bin")],
            hard_deny_reads: vec![PathBuf::from("/secret")],
        };
        let rendered = render_template(
            AGENT_JAIL_TEMPLATE,
            Path::new("/tmp/worktree"),
            Path::new("/Users/testuser"),
            Path::new("/tmp/.aegis/logs/sessions"),
            &policy,
        )
        .expect("built-in template should render without errors");

        assert!(
            rendered.find("@@").is_none(),
            "rendered template still contains unreplaced placeholder: {:?}",
            rendered.lines().find(|l| l.contains("@@"))
        );
    }
}
