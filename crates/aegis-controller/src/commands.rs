use std::sync::Arc;

use crate::clarification::{ClarificationRequest, ClarificationService, ClarifierSource};
use crate::messaging::{MessageDeliveryReceipt, MessageInbox, MessageInboxSummary, MessageRouter};
use aegis_core::{
    AegisError, Agent, AgentRegistry, LogQuery, Recorder, Result, TaskCreator, TaskQueue,
    TaskRegistry,
};
use aegis_design::RenderedTemplate;
use aegis_taskflow::model::{Milestone, ProjectIndex, TaskDraft, TaskPatch};
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
    message_router: Arc<MessageRouter>,
    clarifications: Arc<ClarificationService>,
    scheduler: Arc<Scheduler>,
    recorder: Option<Arc<dyn Recorder>>,
    taskflow: Option<Arc<TaskflowEngine>>,
}

impl ControllerCommands {
    pub fn new(
        registry: Arc<FileRegistry>,
        dispatcher: Arc<Dispatcher>,
        message_router: Arc<MessageRouter>,
        clarifications: Arc<ClarificationService>,
        scheduler: Arc<Scheduler>,
        recorder: Option<Arc<dyn Recorder>>,
        taskflow: Option<Arc<TaskflowEngine>>,
    ) -> Self {
        Self {
            registry,
            dispatcher,
            message_router,
            clarifications,
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

    pub async fn spawn_from_template(&self, rendered: RenderedTemplate) -> Result<Agent> {
        self.dispatcher.spawn_from_template(rendered).await
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

    pub async fn terminate_agent(&self, agent_id: Uuid) -> Result<()> {
        self.dispatcher.terminate_agent(agent_id).await
    }

    pub async fn failover(&self, agent_id: Uuid) -> Result<Agent> {
        let (agent, _) = self.dispatcher.failover_agent(agent_id).await?;
        Ok(agent)
    }

    pub fn resolve_agent_id(&self, raw: &str) -> Result<Uuid> {
        if raw == "self" {
            if let Ok(id_str) = std::env::var("AEGIS_AGENT_ID") {
                if let Ok(uuid) = Uuid::parse_str(&id_str) {
                    return Ok(uuid);
                }
            }
            return Err(AegisError::IpcProtocol {
                reason: "agent_id 'self' specified but AEGIS_AGENT_ID env var is missing or invalid"
                    .to_string(),
            });
        }

        let agents = self.list_all_agents()?;
        resolve_agent_id_from_agents(&agents, raw)
    }

    pub async fn send_message(
        &self,
        from_agent_id: Option<Uuid>,
        to_agent_raw: &str,
        kind: aegis_core::MessageType,
        payload: serde_json::Value,
    ) -> Result<MessageDeliveryReceipt> {
        self.message_router
            .send(from_agent_id, to_agent_raw, kind, payload)
            .await
    }

    pub fn inbox(&self, agent_raw: &str) -> Result<MessageInbox> {
        self.message_router.inbox(agent_raw)
    }

    pub fn list_inboxes(&self) -> Result<Vec<MessageInboxSummary>> {
        self.message_router.list()
    }

    pub fn clarify_request(
        &self,
        agent_raw: &str,
        task_id: Option<Uuid>,
        question: &str,
        context: serde_json::Value,
        priority: i32,
    ) -> Result<ClarificationRequest> {
        self.clarifications
            .request(agent_raw, task_id, question, context, priority)
    }

    pub fn clarify_list(&self) -> Result<Vec<ClarificationRequest>> {
        self.clarifications.list()
    }

    pub fn clarify_list_for_agent(&self, agent_raw: &str) -> Result<Vec<ClarificationRequest>> {
        self.clarifications.list_for_agent(agent_raw)
    }

    pub fn clarify_show(&self, request_id: Uuid) -> Result<ClarificationRequest> {
        self.clarifications.show(request_id)
    }

    pub fn clarify_resolve_request_id(&self, raw: &str) -> Result<Uuid> {
        self.clarifications.resolve_request_id(raw)
    }

    pub async fn clarify_answer(
        &self,
        request_id: Uuid,
        answer: &str,
        payload: serde_json::Value,
        answered_by: ClarifierSource,
    ) -> Result<ClarificationRequest> {
        self.clarifications
            .answer(request_id, answer, payload, answered_by)
            .await
    }

    pub async fn clarify_wait(
        &self,
        target: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<ClarificationRequest> {
        self.clarifications.wait(target, timeout).await
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

    pub fn taskflow_assign(&self, roadmap_id: &str, task_or_agent_id: Uuid) -> Result<()> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        let task_id = resolve_registry_task_id_for_assignment(self.registry.as_ref(), task_or_agent_id)?;
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

    pub fn taskflow_create_task(
        &self,
        milestone_id: &str,
        draft: TaskDraft,
    ) -> Result<aegis_taskflow::model::ProjectTask> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.create_task(milestone_id, draft)
    }

    pub fn taskflow_update_task(
        &self,
        source_milestone_id: &str,
        task_uid: Uuid,
        patch: TaskPatch,
    ) -> Result<aegis_taskflow::model::ProjectTask> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.update_task(source_milestone_id, task_uid, patch)
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

    pub fn taskflow_next(&self) -> Result<aegis_taskflow::NextMilestoneOutcome> {
        let tf = self.taskflow.as_ref().ok_or_else(|| AegisError::Config {
            field: "taskflow".to_string(),
            reason: "Taskflow engine is not initialized".to_string(),
        })?;
        tf.next_milestone()
    }

    /// Sends a `roadmap_updated` notification to all active bastion agents.
    /// Returns the count of agents notified (0 if no bastion is running — not an error).
    pub async fn taskflow_notify(&self, event: &str, message: &str) -> Result<usize> {
        notify_active_bastions(self.registry.as_ref(), &self.message_router, event, message).await
    }

    pub async fn worktree_create(&self, milestone_id: &str) -> Result<std::path::PathBuf> {
        self.dispatcher.worktree_create(milestone_id).await
    }

    pub async fn worktree_merge(&self, milestone_id: &str) -> Result<()> {
        self.dispatcher.worktree_merge(milestone_id).await
    }

    pub async fn worktree_list(&self) -> Result<Vec<(String, std::path::PathBuf)>> {
        self.dispatcher.worktree_list().await
    }
}

async fn notify_active_bastions(
    registry: &FileRegistry,
    router: &MessageRouter,
    event: &str,
    message: &str,
) -> Result<usize> {
    let bastions: Vec<_> = AgentRegistry::list_by_role(registry, "bastion")?
        .into_iter()
        .filter(|a| a.status == aegis_core::AgentStatus::Active)
        .collect();

    for bastion in &bastions {
        router
            .send(
                None,
                &bastion.agent_id.to_string(),
                aegis_core::MessageType::Notification,
                serde_json::json!({ "event": event, "message": message }),
            )
            .await?;
    }

    Ok(bastions.len())
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

fn resolve_registry_task_id_for_assignment(
    registry: &FileRegistry,
    task_or_agent_id: Uuid,
) -> Result<Uuid> {
    if TaskRegistry::get(registry, task_or_agent_id)?.is_some() {
        return Ok(task_or_agent_id);
    }

    if let Some(agent) = AgentRegistry::get(registry, task_or_agent_id)? {
        return agent.task_id.ok_or_else(|| AegisError::IpcProtocol {
            reason: format!("Agent {task_or_agent_id} does not have an assigned task"),
        });
    }

    Err(AegisError::TaskNotFound {
        task_id: task_or_agent_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{AgentKind, AgentStatus, Task, TaskStatus};
    use chrono::Utc;
    use tempfile::tempdir;
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

    #[test]
    fn assignment_id_resolves_splinter_agent_to_registry_task() {
        let dir = tempdir().unwrap();
        let (registry, _router) = setup(dir.path());
        let agent_id = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000005").unwrap();
        let task_id = Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000005").unwrap();
        let now = Utc::now();
        let task = Task {
            task_id,
            description: "Implement task".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent_id),
            created_by: TaskCreator::System,
            created_at: now,
            completed_at: None,
            receipt_path: None,
        };
        let mut splinter = agent(agent_id);
        splinter.task_id = Some(task_id);

        TaskRegistry::insert(registry.as_ref(), &task).unwrap();
        AgentRegistry::insert(registry.as_ref(), &splinter).unwrap();

        let resolved = resolve_registry_task_id_for_assignment(registry.as_ref(), agent_id).unwrap();

        assert_eq!(resolved, task_id);
    }

    // --- notify_active_bastions tests ---

    fn write_minimal_config(root: &std::path::Path) {
        let config = "[providers.claude-code]\nbinary = \"claude-code\"\n\n[splinter_defaults]\ncli_provider = \"claude-code\"\n";
        std::fs::write(root.join("aegis.toml"), config).unwrap();
    }

    fn make_bastion(agent_id: Uuid, status: AgentStatus) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id,
            name: format!("bastion-{agent_id}"),
            kind: AgentKind::Bastion,
            status,
            role: "bastion".to_string(),
            parent_id: None,
            task_id: None,
            tmux_session: "aegis".to_string(),
            tmux_window: 0,
            tmux_pane: "%0".to_string(),
            worktree_path: "/tmp/worktree".into(),
            cli_provider: "claude-code".to_string(),
            fallback_cascade: vec![],
            sandbox_profile: "/tmp/profile.sb".into(),
            log_path: "/tmp/log.log".into(),
            created_at: now,
            updated_at: now,
            terminated_at: None,
        }
    }

    fn setup(root: &std::path::Path) -> (Arc<crate::registry::FileRegistry>, crate::messaging::MessageRouter) {
        write_minimal_config(root);
        let storage = Arc::new(crate::storage::ProjectStorage::new(root.to_path_buf()));
        storage.ensure_layout().unwrap();
        crate::registry::FileRegistry::init(storage.as_ref()).unwrap();
        let registry = Arc::new(crate::registry::FileRegistry::new(storage.clone()));
        let router = crate::messaging::MessageRouter::new(Arc::clone(&registry), storage, None);
        (registry, router)
    }

    #[tokio::test]
    async fn notify_sends_to_active_bastion_and_returns_one() {
        let dir = tempdir().unwrap();
        let (registry, router) = setup(dir.path());
        let bastion_id = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000001").unwrap();
        AgentRegistry::insert(registry.as_ref(), &make_bastion(bastion_id, AgentStatus::Active)).unwrap();

        let count = notify_active_bastions(registry.as_ref(), &router, "roadmap_updated", "new milestone added").await.unwrap();

        assert_eq!(count, 1);
        let inbox = router.inbox(&bastion_id.to_string()).unwrap();
        assert_eq!(inbox.messages.len(), 1);
        assert_eq!(inbox.messages[0].payload["event"], "roadmap_updated");
    }

    #[tokio::test]
    async fn notify_with_no_active_bastion_returns_zero() {
        let dir = tempdir().unwrap();
        let (registry, router) = setup(dir.path());
        let bastion_id = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000002").unwrap();
        AgentRegistry::insert(registry.as_ref(), &make_bastion(bastion_id, AgentStatus::Terminated)).unwrap();

        let count = notify_active_bastions(registry.as_ref(), &router, "roadmap_updated", "").await.unwrap();

        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn notify_routes_to_all_active_bastions() {
        let dir = tempdir().unwrap();
        let (registry, router) = setup(dir.path());
        let b1 = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000003").unwrap();
        let b2 = Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000004").unwrap();
        AgentRegistry::insert(registry.as_ref(), &make_bastion(b1, AgentStatus::Active)).unwrap();
        AgentRegistry::insert(registry.as_ref(), &make_bastion(b2, AgentStatus::Active)).unwrap();

        let count = notify_active_bastions(registry.as_ref(), &router, "roadmap_updated", "two bastions").await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(router.inbox(&b1.to_string()).unwrap().messages.len(), 1);
        assert_eq!(router.inbox(&b2.to_string()).unwrap().messages.len(), 1);
    }
}
