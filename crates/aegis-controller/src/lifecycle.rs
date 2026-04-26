use std::{
    path::{Path, PathBuf},
    process::Command,
};

use aegis_core::{
    config::{NetworkPolicy, SandboxPolicyConfig},
    Agent, AgentKind, AgentStatus, SandboxNetworkPolicy, SandboxPolicy,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub kind: AgentKind,
    pub role: String,
    pub parent_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub task_description: Option<String>,
    pub cli_provider: String,
    pub fallback_cascade: Vec<String>,
    pub system_prompt: Option<PathBuf>,
    pub sandbox: SandboxPolicy,
    pub auto_cleanup: bool,
}

#[derive(Debug)]
pub struct SpawnPlan {
    pub agent: Agent,
    pub provider_command: Command,
    pub launch_command: Vec<String>,
    pub initial_prompt: String,
    pub sandbox_policy: SandboxPolicy,
}

#[derive(Debug, Clone)]
pub struct RunningAgent {
    agent: Agent,
}

impl RunningAgent {
    pub fn new(agent: Agent) -> Self {
        Self { agent }
    }

    pub fn agent(&self) -> &Agent {
        &self.agent
    }
}

impl aegis_core::AgentHandle for RunningAgent {
    fn agent_id(&self) -> Uuid {
        self.agent.agent_id
    }

    fn tmux_target(&self) -> String {
        self.agent.tmux_target()
    }

    fn worktree_path(&self) -> &Path {
        &self.agent.worktree_path
    }

    fn is_alive(&self) -> bool {
        !self.agent.status.is_terminal()
    }
}

pub fn sandbox_policy_from_config(config: &SandboxPolicyConfig) -> SandboxPolicy {
    SandboxPolicy {
        network: match config.network {
            NetworkPolicy::None => SandboxNetworkPolicy::None,
            NetworkPolicy::OutboundOnly => SandboxNetworkPolicy::OutboundOnly,
            NetworkPolicy::Any => SandboxNetworkPolicy::Any,
        },
        extra_reads: config.extra_reads.clone(),
        extra_writes: config.extra_writes.clone(),
        extra_exec_paths: config.extra_exec_paths.clone(),
        hard_deny_reads: Vec::new(),
    }
}

pub fn validate_transition(from: &AgentStatus, to: &AgentStatus) -> bool {
    use AgentStatus::*;

    matches!(
        (from, to),
        (Queued, Starting)
            | (Starting, Active)
            | (Starting, Failed)
            | (Active, Paused)
            | (Active, Cooling)
            | (Active, Reporting)
            | (Active, Failed)
            | (Paused, Active)
            | (Paused, Failed)
            | (Cooling, Active)
            | (Cooling, Failed)
            | (Reporting, Terminated)
            | (Reporting, Failed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_policy_maps_network_variants() {
        let policy = sandbox_policy_from_config(&SandboxPolicyConfig {
            network: NetworkPolicy::None,
            extra_reads: vec![PathBuf::from("/read")],
            extra_writes: vec![PathBuf::from("/write")],
            extra_exec_paths: vec![PathBuf::from("/exec")],
        });

        assert_eq!(policy.network, SandboxNetworkPolicy::None);
        assert_eq!(policy.extra_reads, vec![PathBuf::from("/read")]);
        assert_eq!(policy.extra_writes, vec![PathBuf::from("/write")]);
    }

    #[test]
    fn lifecycle_transition_table_rejects_invalid_edges() {
        assert!(validate_transition(
            &AgentStatus::Queued,
            &AgentStatus::Starting
        ));
        assert!(validate_transition(
            &AgentStatus::Cooling,
            &AgentStatus::Active
        ));
        assert!(!validate_transition(
            &AgentStatus::Terminated,
            &AgentStatus::Active
        ));
        assert!(!validate_transition(
            &AgentStatus::Queued,
            &AgentStatus::Terminated
        ));
    }
}
