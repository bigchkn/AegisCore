use std::sync::Arc;

use aegis_core::{Agent, AgentRegistry, LogQuery, Recorder, Result, TaskCreator, TaskQueue};
use serde::Serialize;
use uuid::Uuid;

use crate::{dispatcher::Dispatcher, registry::FileRegistry, scheduler::Scheduler};

#[derive(Debug, Clone, Serialize)]
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
}

impl ControllerCommands {
    pub fn new(
        registry: Arc<FileRegistry>,
        dispatcher: Arc<Dispatcher>,
        scheduler: Arc<Scheduler>,
        recorder: Option<Arc<dyn Recorder>>,
    ) -> Self {
        Self {
            registry,
            dispatcher,
            scheduler,
            recorder,
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

    pub fn spawn(&self, task: &str) -> Result<Uuid> {
        self.scheduler
            .enqueue_splinter_task(task, TaskCreator::System)
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
}
