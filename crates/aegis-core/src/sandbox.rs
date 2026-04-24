use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkPolicy {
    None,
    #[default]
    OutboundOnly,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxPolicy {
    pub network: SandboxNetworkPolicy,
    pub extra_reads: Vec<PathBuf>,
    pub extra_writes: Vec<PathBuf>,
    /// Paths explicitly denied even if a parent subpath rule would allow them.
    pub hard_deny_reads: Vec<PathBuf>,
}

pub trait SandboxProfile: Send + Sync {
    /// Render the `.sb` profile content as a string.
    fn render(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy) -> Result<String>;

    /// Render and write the profile to `dest` atomically.
    fn write(
        &self,
        worktree: &Path,
        home: &Path,
        policy: &SandboxPolicy,
        dest: &Path,
    ) -> Result<()>;

    /// Returns the argument list to prefix any command for sandbox execution.
    /// e.g. `["sandbox-exec", "-f", "/path/to/profile.sb"]`
    fn exec_prefix(&self, profile_path: &Path) -> Vec<String>;
}
