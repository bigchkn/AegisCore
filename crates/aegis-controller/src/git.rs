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

    pub fn milestone_branch(milestone_id: &str) -> String {
        format!("aegis/milestone/{milestone_id}")
    }

    pub fn path_for_milestone(&self, milestone_id: &str) -> PathBuf {
        self.worktrees_dir.join(format!("milestone-{milestone_id}"))
    }

    /// Creates a worktree for a milestone at `.aegis/worktrees/milestone-<id>/`
    /// on branch `aegis/milestone/<id>`. Idempotent: if the path already exists
    /// the existing path is returned without error.
    pub async fn create_for_milestone(&self, milestone_id: &str) -> Result<PathBuf> {
        let path = self.path_for_milestone(milestone_id);
        if path.exists() {
            return Ok(path);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let branch = Self::milestone_branch(milestone_id);
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

    /// Merges `aegis/milestone/<id>` into the current branch (main) using
    /// --no-ff. On merge conflict aborts and returns `GitMergeConflict`.
    /// On success removes the worktree and prunes the branch.
    pub async fn merge_milestone_into_main(&self, milestone_id: &str) -> Result<()> {
        let branch = Self::milestone_branch(milestone_id);

        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("merge")
            .arg("--no-ff")
            .arg(&branch)
            .arg("-m")
            .arg(format!("chore: merge milestone {milestone_id}"))
            .output()
            .await
            .map_err(|source| AegisError::StorageIo {
                path: self.project_root.clone(),
                source,
            })?;

        if !output.status.success() {
            // Abort the failed merge to restore clean state.
            let _ = tokio::process::Command::new("git")
                .arg("-C")
                .arg(&self.project_root)
                .arg("merge")
                .arg("--abort")
                .output()
                .await;

            return Err(AegisError::GitMergeConflict {
                branch,
                reason: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        // Remove the worktree and branch.
        let path = self.path_for_milestone(milestone_id);
        if path.exists() {
            run_git_worktree_remove(&self.project_root, &path).await?;
        }

        let _ = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("branch")
            .arg("-d")
            .arg(&branch)
            .output()
            .await;

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

    /// Returns (milestone_id, worktree_path) for all active milestone worktrees.
    pub async fn list_milestone_worktrees(&self) -> Result<Vec<(String, PathBuf)>> {
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.project_root)
            .arg("worktree")
            .arg("list")
            .arg("--porcelain")
            .output()
            .await
            .map_err(|source| AegisError::StorageIo {
                path: self.project_root.clone(),
                source,
            })?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        let mut current_path: Option<PathBuf> = None;

        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(rest));
            } else if let Some(rest) = line.strip_prefix("branch refs/heads/aegis/milestone/") {
                if let Some(path) = current_path.take() {
                    results.push((rest.to_string(), path));
                }
            }
        }

        Ok(results)
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

    #[test]
    fn milestone_branch_name_is_correct() {
        assert_eq!(GitWorktree::milestone_branch("M31"), "aegis/milestone/M31");
        assert_eq!(GitWorktree::milestone_branch("30"), "aegis/milestone/30");
    }

    #[test]
    fn milestone_worktree_path_is_correct() {
        let git = GitWorktree::new(
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.aegis/worktrees"),
        );
        assert_eq!(
            git.path_for_milestone("M31"),
            PathBuf::from("/repo/.aegis/worktrees/milestone-M31")
        );
    }

    #[test]
    fn milestone_path_does_not_collide_with_agent_path() {
        let git = GitWorktree::new(
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.aegis/worktrees"),
        );
        let agent_path = git.path_for_agent(Uuid::nil());
        let milestone_path = git.path_for_milestone("M31");
        assert_ne!(agent_path, milestone_path);
    }
}
