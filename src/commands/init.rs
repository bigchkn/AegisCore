use std::path::{Path, PathBuf};
use crate::{anchoring::ProjectAnchor, client::DaemonClient, error::AegisCliError, output::Printer};

const GITIGNORE_BLOCK: &str = r#"
# AegisCore runtime directories
.aegis/logs/
.aegis/state/
.aegis/channels/
.aegis/profiles/
.aegis/worktrees/
.aegis/handoff/
"#;

const DEFAULT_AEGIS_TOML: &str = r#"[global]
max_splinters = 5
tmux_session_name = "aegis"
telegram_enabled = false

[watchdog]
poll_interval_ms = 2000
scan_lines = 50
failover_enabled = true

[recorder]
failover_context_lines = 100
log_rotation_max_mb = 50
log_retention_count = 20

[state]
snapshot_interval_s = 60
snapshot_retention_count = 10

[sandbox.defaults]
network = "outbound_only"
extra_reads = []
extra_writes = []

[providers.claude-code]
binary = "claude"
resume_flag = "--resume"

[providers.gemini-cli]
binary = "gemini"

[providers.ollama]
binary = "ollama"
model = "gemma3"

[splinter_defaults]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "ollama"]
auto_cleanup = true
"#;

pub async fn run(
    force: bool,
    printer: &Printer,
    client: &DaemonClient,
) -> Result<(), AegisCliError> {
    let cwd = std::env::current_dir()?;
    let anchor = ProjectAnchor::use_cwd(&cwd);
    let aegis_dir = &anchor.aegis_dir;

    if aegis_dir.exists() && !force {
        return Err(AegisCliError::InvalidArg(format!(
            "Already an AegisCore project at {}.\nRun with --force to reinitialize.",
            cwd.display()
        )));
    }

    scaffold_directories(aegis_dir)?;
    write_aegis_toml(&cwd)?;
    scaffold_prompts(aegis_dir)?;
    update_gitignore(&cwd)?;

    // Best-effort daemon registration — don't fail init if daemon is down.
    match client
        .request(None, "projects.register", serde_json::json!({ "root_path": cwd }))
        .await
    {
        Ok(_) => {}
        Err(AegisCliError::DaemonNotRunning) => {
            printer.warn(
                "aegisd is not running — project registered locally only.\n\
                 Run 'aegis daemon start' to start the daemon.",
            );
        }
        Err(e) => {
            printer.warn(&format!("Could not register with daemon: {e}"));
        }
    }

    printer.line(&format!(
        "Initialized AegisCore project in {}\n\
         Edit aegis.toml to configure agents and providers.\n\
         Run 'aegis daemon start' then 'aegis start' to launch Bastion agents.",
        aegis_dir.display()
    ));
    Ok(())
}

fn scaffold_directories(aegis_dir: &Path) -> Result<(), AegisCliError> {
    let dirs = [
        "state",
        "logs/sessions",
        "logs/archive",
        "channels",
        "profiles",
        "worktrees",
        "handoff",
        "designs/hld",
        "designs/lld",
        "prompts/system",
        "prompts/task",
        "prompts/handoff",
    ];
    for d in &dirs {
        std::fs::create_dir_all(aegis_dir.join(d))?;
    }
    Ok(())
}

fn write_aegis_toml(project_root: &Path) -> Result<(), AegisCliError> {
    let toml_path = project_root.join("aegis.toml");
    if toml_path.exists() {
        return Ok(()); // preserve any existing toml
    }

    // Try to read ~/.aegis/config as seed; fall back to built-in default.
    let content = match read_global_config_raw() {
        Some(raw) => raw,
        None => DEFAULT_AEGIS_TOML.to_string(),
    };

    std::fs::write(&toml_path, content)?;
    Ok(())
}

fn read_global_config_raw() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".aegis").join("config");
    std::fs::read_to_string(path).ok()
}

fn scaffold_prompts(aegis_dir: &Path) -> Result<(), AegisCliError> {
    let system_default = include_str!(
        "../../crates/aegis-controller/src/prompts/templates/system_default.md"
    );
    let recovery_default = include_str!(
        "../../crates/aegis-controller/src/prompts/templates/recovery_default.md"
    );
    let resume_default = include_str!(
        "../../crates/aegis-controller/src/prompts/templates/resume_default.md"
    );

    write_if_absent(aegis_dir.join("prompts/system/default.md"), system_default)?;
    write_if_absent(aegis_dir.join("prompts/handoff/recovery.md"), recovery_default)?;
    write_if_absent(aegis_dir.join("prompts/handoff/resume.md"), resume_default)?;
    Ok(())
}

fn write_if_absent(path: PathBuf, content: &str) -> Result<(), AegisCliError> {
    if !path.exists() {
        std::fs::write(path, content)?;
    }
    Ok(())
}

fn update_gitignore(project_root: &Path) -> Result<(), AegisCliError> {
    let gi_path = project_root.join(".gitignore");
    let existing = std::fs::read_to_string(&gi_path).unwrap_or_default();

    if existing.contains(".aegis/logs/") {
        return Ok(()); // already patched
    }

    let mut content = existing;
    content.push_str(GITIGNORE_BLOCK);
    std::fs::write(gi_path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn scaffold_creates_all_dirs() {
        let tmp = make_tmp();
        let aegis_dir = tmp.path().join(".aegis");
        scaffold_directories(&aegis_dir).unwrap();
        assert!(aegis_dir.join("state").is_dir());
        assert!(aegis_dir.join("logs/sessions").is_dir());
        assert!(aegis_dir.join("prompts/handoff").is_dir());
        assert!(aegis_dir.join("designs/lld").is_dir());
    }

    #[test]
    fn writes_aegis_toml() {
        let tmp = make_tmp();
        write_aegis_toml(tmp.path()).unwrap();
        let toml_path = tmp.path().join("aegis.toml");
        assert!(toml_path.exists());
        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("max_splinters"));
    }

    #[test]
    fn does_not_overwrite_existing_toml() {
        let tmp = make_tmp();
        let toml_path = tmp.path().join("aegis.toml");
        std::fs::write(&toml_path, "# existing").unwrap();
        write_aegis_toml(tmp.path()).unwrap();
        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert_eq!(content, "# existing");
    }

    #[test]
    fn gitignore_appended() {
        let tmp = make_tmp();
        std::fs::write(tmp.path().join(".gitignore"), "target/\n").unwrap();
        update_gitignore(tmp.path()).unwrap();
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains(".aegis/logs/"));
        assert!(content.contains("target/"));
    }

    #[test]
    fn gitignore_not_doubled() {
        let tmp = make_tmp();
        std::fs::write(tmp.path().join(".gitignore"), GITIGNORE_BLOCK).unwrap();
        update_gitignore(tmp.path()).unwrap();
        let content = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content.matches(".aegis/logs/").count(), 1);
    }
}
