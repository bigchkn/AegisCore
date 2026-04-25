use std::collections::HashMap;

use crate::client::ProjectRecord;
use aegis_core::{AegisEvent, Agent, AgentStatus, ChannelRecord, Task};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum PaneMode {
    Normal,
    Command,
    Input, // Interactive terminal input
}

#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    None,
    Help,
    ProjectSwitcher {
        projects: Vec<ProjectRecord>,
        selected_idx: usize,
    },
    SpawnPrompt {
        input: String,
    },
    ConfirmKill {
        agent_id: Uuid,
    },
}

pub struct AppState {
    pub project_path: std::path::PathBuf,
    pub projects: Vec<ProjectRecord>,
    pub agents: HashMap<Uuid, Agent>,
    pub tasks: Vec<Task>,
    pub channels: Vec<ChannelRecord>,
    pub selected_agent_id: Option<Uuid>,
    pub mode: PaneMode,
    pub overlay: Overlay,
    pub logs: HashMap<Uuid, Vec<String>>,
    pub connection_status: ConnectionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl AppState {
    pub fn new(project_path: std::path::PathBuf) -> Self {
        Self {
            project_path,
            projects: Vec::new(),
            agents: HashMap::new(),
            tasks: Vec::new(),
            channels: Vec::new(),
            selected_agent_id: None,
            mode: PaneMode::Normal,
            overlay: Overlay::None,
            logs: HashMap::new(),
            connection_status: ConnectionStatus::Disconnected,
        }
    }

    pub fn handle_aegis_event(&mut self, event: AegisEvent) {
        match event {
            AegisEvent::AgentSpawned {
                agent_id: _,
                role: _,
            } => {
                // We'll need a full refresh to get the Agent record
            }
            AegisEvent::AgentStatusChanged {
                agent_id,
                new_status,
                ..
            } => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.status = new_status;
                }
            }
            AegisEvent::AgentTerminated { agent_id, .. } => {
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.status = AgentStatus::Terminated;
                }
            }
            AegisEvent::TaskAssigned { task_id, agent_id } => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.task_id == task_id) {
                    task.assigned_agent_id = Some(agent_id);
                }
                if let Some(agent) = self.agents.get_mut(&agent_id) {
                    agent.task_id = Some(task_id);
                }
            }
            AegisEvent::TaskComplete { task_id, .. } => {
                if let Some(task) = self.tasks.iter_mut().find(|t| t.task_id == task_id) {
                    task.status = aegis_core::TaskStatus::Complete;
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub enum TuiEvent {
    Aegis(AegisEvent),
    Key(crossterm::event::KeyEvent),
    Tick,
    RefreshRequested,
}
