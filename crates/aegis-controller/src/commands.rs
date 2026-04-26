use std::sync::Arc;

use aegis_core::{
    AegisError, Agent, AgentRegistry, LogQuery, Recorder, Result, TaskCreator, TaskQueue,
};
use aegis_taskflow::model::{Milestone, ProjectIndex};
use aegis_taskflow::TaskflowEngine;
use serde::Serialize;
use uuid::Uuid;

use crate::{dispatcher::Dispatcher, registry::FileRegistry, scheduler::Scheduler};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ProjectStatus {
    pub active_agents: usize,
    pub pending_tasks: usize,
}

#[derive(Clone)]
pub struct ControllerCommands {
    registry: Arc<FileRegistry>,
    dispatcher: Arc<Dispatcher>,
    scheduler: Arc<Scheduler>,
    recorder: Option<Arc<dyn Recorder>>,
    taskflow: Option<Arc<TaskflowEngine>>,
}

impl ControllerCommands {
    pub fn new(
        registry: Arc<FileRegistry>,
        dispatcher: Arc<Dispatcher>,
        scheduler: Arc<Scheduler>,
        recorder: Option<Arc<dyn Recorder>>,
        taskflow: Option<Arc<TaskflowEngine>>,
    ) -> Self {
        Self {
            registry,
            dispatcher,
            scheduler,
            recorder,
            taskflow,
        }
    }

    pub fn status(&self) -> Result<ProjectStatus> {
        Ok(ProjectStatus {
            active_agents: AgentRegistry::list_active(self.registry.as_ref())?.len(),
            pending_tasks: TaskQueue::pending_count(self.registry.as_ref())?,
        })
    }

    pub fn list_agents(&self) -> Result<Vec<Agent>> {
        AgentRegistry::list_active(self.registry.as_ref())
    }

    pub fn list_all_agents(&self) -> Result<Vec<Agent>> {
        AgentRegistry::list_all(self.registry.as_ref())
    }

    pub fn list_tasks(&self) -> Result<Vec<aegis_core::Task>> {
        aegis_core::TaskRegistry::list_all(self.registry.as_ref())
    }

    pub fn list_channels(&self) -> Result<Vec<aegis_core::ChannelRecord>> {
        aegis_core::ChannelRegistry::list(self.registry.as_ref())
    }

    pub async fn spawn(&self, task: &str) -> Result<Uuid> {
        let task_id = self
            .scheduler
            .enqueue_splinter_task(task, TaskCreator::System)?;
        // Attempt immediate dispatch; if max_splinters is saturated the task
        // stays queued until a slot opens (no-op on None return).
        if let Err(e) = self.scheduler.dispatch_once("splinter").await {
            tracing::error!(task_id = %task_id, error = %e, "dispatch failed after enqueue — task remains queued");
        }
        Ok(task_id)
    }

    pub async fn pause(&self, agent_id: Uuid) -> Result<()> {
        self.dispatcher.pause_agent(agent_id).await
    }

    pub async fn resume(&self, agent_id: Uuid) -> Result<()> {
        self.dispatcher.resume_agent(agent_id).await
    }

    pub async fn kill(&self, agent_id: Uuid) -> Result<()> {
        self.dispatcher.kill_agent(agent_id).await
    }

    pub async fn failover(&self, agent_id: Uuid) -> Result<Agent> {
        self.dispatcher.failover_agent(agent_id).await
    }

    pub fn resolve_agent_id(&self, raw: &str) -> Result<Uuid> {
        let agents = self.list_all_agents()?;
        resolve_agent_id_from_agents(&agents, raw)
    }

    pub fn logs(&self, agent_id: Uuid, lines: Option<usize>) -> Result<Vec<String>> {
        let Some(recorder) = &self.recorder else {
            return Ok(Vec::new());
        };
        recorder.query(&LogQuery {
            agent_id,
            last_n_lines: lines,
            since: None,
            follow: false,
        })
    }

    pub fn taskflow_status(&self) -> Result<ProjectIndex> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.get_status()
    }

    pub fn taskflow_show(&self, milestone_id: &str) -> Result<Milestone> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;

        tf.get_milestone(milestone_id)
    }

    pub fn taskflow_assign(&self, roadmap_id: &str, task_id: Uuid) -> Result<()> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.links().assign(roadmap_id.to_string(), task_id)
    }

    pub fn taskflow_sync(&self) -> Result<aegis_taskflow::SyncReport> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.sync()
    }

    pub fn taskflow_create_milestone(&self, id: &str, name: &str, lld: Option<&str>) -> Result<()> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.create_milestone(id, name, lld)
    }

    pub fn taskflow_add_task(
        &self,
        milestone_id: &str,
        task_id: &str,
        task: &str,
        task_type: aegis_taskflow::model::TaskType,
    ) -> Result<()> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.add_task(milestone_id, task_id, task, task_type)
    }

    pub fn taskflow_set_task_status(
        &self,
        milestone_id: &str,
        task_id: &str,
        status: &str,
    ) -> Result<()> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.set_task_status(milestone_id, task_id, status)
    }
}

fn resolve_agent_id_from_agents(agents: &[Agent], raw: &str) -> Result<Uuid> {
    if let Ok(uuid) = Uuid::parse_str(raw) {
        if agents.iter().any(|agent| agent.agent_id == uuid) {
            return Ok(uuid);
        }
        return Err(AegisError::AgentNotFound { agent_id: uuid });
    }

    let matches: Vec<Uuid> = agents
        .iter()
        .filter(|agent| agent.agent_id.to_string().starts_with(raw))
        .map(|agent| agent.agent_id)
        .collect();

    match matches.as_slice() {
        [agent_id] => Ok(*agent_id),
        [] => Err(AegisError::IpcProtocol {
            reason: format!("Unknown agent_id prefix `{raw}`"),
        }),
        _ => Err(AegisError::IpcProtocol {
            reason: format!("Ambiguous agent_id prefix `{raw}`"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{AgentKind, AgentStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn agent(agent_id: Uuid) -> Agent {
        Agent {
            agent_id,
            name: format!("agent-{agent_id}"),
            kind: AgentKind::Splinter,
            status: AgentStatus::Active,
            role: "splinter".to_string(),
            parent_id: None,
            task_id: None,
            tmux_session: "aegis".to_string(),
            tmux_window: 0,
            tmux_pane: "0".to_string(),
            worktree_path: "/tmp/worktree".into(),
            cli_provider: "claude-code".to_string(),
            fallback_cascade: vec!["codex".to_string()],
            sandbox_profile: "/tmp/profile.sb".into(),
            log_path: "/tmp/log.log".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            terminated_at: None,
        }
    }

    #[test]
    fn resolves_unique_prefix() {
        let agent_id = Uuid::parse_str("603685e0-1111-2222-3333-444444444444").unwrap();
        let agents = vec![agent(agent_id)];

        let resolved = resolve_agent_id_from_agents(&agents, "603685e0").unwrap();

        assert_eq!(resolved, agent_id);
    }

    #[test]
    fn rejects_ambiguous_prefix() {
        let agents = vec![
            agent(Uuid::parse_str("603685e0-1111-2222-3333-444444444444").unwrap()),
            agent(Uuid::parse_str("603685e0-aaaa-bbbb-cccc-dddddddddddd").unwrap()),
        ];

        let err = resolve_agent_id_from_agents(&agents, "603685e0").unwrap_err();

        assert!(err.to_string().contains("Ambiguous agent_id prefix"));
    }

    #[test]
    fn exact_uuid_requires_registry_membership() {
        let agent_id = Uuid::parse_str("603685e0-1111-2222-3333-444444444444").unwrap();
        let err = resolve_agent_id_from_agents(&[], &agent_id.to_string()).unwrap_err();

        assert!(matches!(err, AegisError::AgentNotFound { agent_id: id } if id == agent_id));
    }
}
