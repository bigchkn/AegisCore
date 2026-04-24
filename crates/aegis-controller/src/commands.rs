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
}
