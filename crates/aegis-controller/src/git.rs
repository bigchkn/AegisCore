use std::path::{Path, PathBuf};

use aegis_core::{AegisError, Result};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct GitWorktree {
    project_root: PathBuf,
    worktrees_dir: PathBuf,
}

impl GitWorktree {
    pub fn new(project_root: PathBuf, worktrees_dir: PathBuf) -> Self {
        Self {
            project_root,
            worktrees_dir,
        }
    }

    pub fn path_for_agent(&self, agent_id: Uuid) -> PathBuf {
        self.worktrees_dir.join(agent_id.to_string())
    }

    pub fn branch_for_agent(role: &str, agent_id: Uuid) -> String {
        format!("aegis/{role}/{}", short_id(agent_id))
    }

    pub async fn create_for_agent(&self, agent_id: Uuid, role: &str) -> Result<PathBuf> {
        let path = self.path_for_agent(agent_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let branch = Self::branch_for_agent(role, agent_id);
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("worktree")
            .arg("add")
            .arg("-B")
            .arg(&branch)
            .arg(&path)
            .arg("HEAD")
            .output()
            .await
            .map_err(|source| AegisError::StorageIo {
                path: self.project_root.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(AegisError::GitWorktreeAdd {
                path,
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(path)
    }

    pub async fn prune_for_agent(&self, agent_id: Uuid) -> Result<()> {
        let path = self.path_for_agent(agent_id);
        if path.exists() {
            run_git_worktree_remove(&self.project_root, &path).await?;
        }

        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("worktree")
            .arg("prune")
            .output()
            .await
            .map_err(|source| AegisError::StorageIo {
                path: self.project_root.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(AegisError::GitWorktreePrune {
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(())
    }
}

async fn run_git_worktree_remove(project_root: &Path, path: &Path) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(project_root)
        .arg("worktree")
        .arg("remove")
        .arg("--force")
        .arg(path)
        .output()
        .await
        .map_err(|source| AegisError::StorageIo {
            path: project_root.to_path_buf(),
            source,
        })?;

    if !output.status.success() {
        return Err(AegisError::GitWorktreePrune {
            reason: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(())
}

fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_and_path_are_deterministic() {
        let agent_id = Uuid::nil();
        let git = GitWorktree::new(
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.aegis/worktrees"),
        );

        assert_eq!(
            GitWorktree::branch_for_agent("worker", agent_id),
            "aegis/worker/00000000"
        );
        assert_eq!(
            git.path_for_agent(agent_id),
            PathBuf::from("/repo/.aegis/worktrees/00000000-0000-0000-0000-000000000000")
        );
    }
}
