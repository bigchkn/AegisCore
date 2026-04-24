use std::{
    fs,
    path::{Path, PathBuf},
};

use aegis_core::{Result, SandboxPolicy, SandboxProfile};
use tracing::debug;

use crate::{
    template::{render_template, AGENT_JAIL_TEMPLATE},
    SandboxError,
};

#[derive(Debug, Clone)]
pub struct ProfileVars {
    pub worktree_path: PathBuf,
    pub home: PathBuf,
    pub aegis_logs_dir: PathBuf,
    pub policy: SandboxPolicy,
}

#[derive(Debug, Clone)]
pub struct SeatbeltSandbox {
    template: &'static str,
    aegis_logs_dir: Option<PathBuf>,
}

impl SeatbeltSandbox {
    pub fn new() -> Self {
        Self {
            template: AGENT_JAIL_TEMPLATE,
            aegis_logs_dir: None,
        }
    }

    pub fn with_logs_dir(aegis_logs_dir: PathBuf) -> Self {
        Self {
            template: AGENT_JAIL_TEMPLATE,
            aegis_logs_dir: Some(aegis_logs_dir),
        }
    }

    fn vars(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy) -> ProfileVars {
        ProfileVars {
            worktree_path: worktree.to_path_buf(),
            home: home.to_path_buf(),
            aegis_logs_dir: self
                .aegis_logs_dir
                .clone()
                .unwrap_or_else(|| worktree.join(".aegis").join("logs").join("sessions")),
            policy: policy.clone(),
        }
    }
}

impl Default for SeatbeltSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxProfile for SeatbeltSandbox {
    fn render(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy) -> Result<String> {
        let vars = self.vars(worktree, home, policy);
        render_template(
            self.template,
            &vars.worktree_path,
            &vars.home,
            &vars.aegis_logs_dir,
            &vars.policy,
        )
        .map_err(Into::into)
    }

    fn write(
        &self,
        worktree: &Path,
        home: &Path,
        policy: &SandboxPolicy,
        dest: &Path,
    ) -> Result<()> {
        let rendered = self.render(worktree, home, policy)?;
        let tmp = dest.with_extension(format!(
            "{}tmp",
            dest.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| format!("{extension}."))
                .unwrap_or_default()
        ));

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|source| SandboxError::WriteError {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        fs::write(&tmp, rendered).map_err(|source| SandboxError::WriteError {
            path: tmp.clone(),
            source,
        })?;
        set_owner_only_permissions(&tmp)?;
        fs::rename(&tmp, dest).map_err(|source| SandboxError::WriteError {
            path: dest.to_path_buf(),
            source,
        })?;

        debug!(profile = %dest.display(), "wrote sandbox profile");
        Ok(())
    }

    fn exec_prefix(&self, profile_path: &Path) -> Vec<String> {
        vec![
            "sandbox-exec".to_string(),
            "-f".to_string(),
            profile_path.to_string_lossy().into_owned(),
        ]
    }
}

fn set_owner_only_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, permissions).map_err(|source| SandboxError::WriteError {
            path: path.to_path_buf(),
            source,
        })?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}
