use crate::daemon::projects::ProjectRegistry;
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::{AegisError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, error, info};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct UdsRequest {
    pub id: Uuid,
    pub project_path: Option<PathBuf>,
    pub command: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UdsResponse {
    pub id: Uuid,
    pub status: String,
    pub payload: serde_json::Value,
    pub error: Option<String>,
}

pub struct UdsServer {
    listener: UnixListener,
    event_bus: Arc<EventBus>,
    project_registry: Arc<ProjectRegistry>,
    active_runtimes: Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
}

impl UdsServer {
    pub async fn bind(
        path: PathBuf,
        event_bus: Arc<EventBus>,
        project_registry: Arc<ProjectRegistry>,
        active_runtimes: Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
    ) -> Result<Self> {
        // Clean up existing socket if any
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }

        let listener =
            UnixListener::bind(path).map_err(|e| AegisError::IpcConnection { source: e })?;

        Ok(Self {
            listener,
            event_bus,
            project_registry,
            active_runtimes,
        })
    }

    pub async fn run(self) {
        info!("UDS Server listening for connections (Unix Domain Socket)");
        loop {
            match self.listener.accept().await {
                Ok((stream, _addr)) => {
                    let bus = Arc::clone(&self.event_bus);
                    let projects = Arc::clone(&self.project_registry);
                    let runtimes = Arc::clone(&self.active_runtimes);
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, bus, projects, runtimes).await {
                            error!("UDS connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("UDS accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_connection(
    stream: UnixStream,
    event_bus: Arc<EventBus>,
    project_registry: Arc<ProjectRegistry>,
    active_runtimes: Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
) -> Result<()> {
    let mut lines = Framed::new(stream, LinesCodec::new());

    while let Some(result) = lines.next().await {
        let line: String = match result {
            Ok(l) => l,
            Err(e) => {
                return Err(AegisError::IpcProtocol {
                    reason: e.to_string(),
                })
            }
        };

        let request: UdsRequest =
            serde_json::from_str(&line).map_err(|e| AegisError::IpcProtocol {
                reason: format!("Invalid JSON: {}", e),
            })?;

        debug!("UDS Request: {} - {}", request.id, request.command);

        if request.command == "subscribe" {
            handle_subscription(lines, event_bus).await;
            return Ok(());
        }

        // Logic to find project and dispatch command
        let payload = match dispatch_command(&request, &project_registry, &active_runtimes).await {
            Ok(p) => p,
            Err(e) => {
                let response = UdsResponse {
                    id: request.id,
                    status: "error".to_string(),
                    payload: serde_json::Value::Null,
                    error: Some(e.to_string()),
                };
                let response_json = serde_json::to_string(&response).unwrap();
                lines.send(response_json).await.ok();
                continue;
            }
        };

        let response = UdsResponse {
            id: request.id,
            status: "success".to_string(),
            payload,
            error: None,
        };

        let response_json =
            serde_json::to_string(&response).map_err(|e| AegisError::IpcProtocol {
                reason: e.to_string(),
            })?;

        lines
            .send(response_json)
            .await
            .map_err(|e| AegisError::IpcConnection {
                source: std::io::Error::new(std::io::ErrorKind::Other, e),
            })?;
    }

    Ok(())
}

async fn dispatch_command(
    request: &UdsRequest,
    project_registry: &ProjectRegistry,
    active_runtimes: &Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
) -> Result<serde_json::Value> {
    let project_path = request
        .project_path
        .as_ref()
        .ok_or_else(|| AegisError::IpcProtocol {
            reason: "Missing project_path".to_string(),
        })?;

    let project = project_registry
        .find_by_path(project_path)?
        .ok_or_else(|| AegisError::ProjectNotInitialized {
            path: project_path.clone(),
        })?;

    let mut runtimes = active_runtimes.lock().await;
    let runtime = if let Some(r) = runtimes.get(&project.id) {
        r
    } else {
        let r = AegisRuntime::load(project.root_path.clone()).await?;
        r.recover().await?;
        runtimes.insert(project.id, r);
        runtimes.get(&project.id).unwrap()
    };

    let commands = runtime.commands();

    match request.command.as_str() {
        "status" => Ok(serde_json::to_value(commands.status()?).unwrap()),
        "agents.list" => Ok(serde_json::to_value(commands.list_agents()?).unwrap()),
        "agents.spawn" => {
            let task = request
                .params
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing task".to_string(),
                })?;
            let task_id = commands.spawn(task)?;
            Ok(serde_json::json!({ "task_id": task_id }))
        }
        "agents.pause" => {
            let agent_id = parse_agent_id(&request.params)?;
            commands.pause(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "paused" }))
        }
        "agents.resume" => {
            let agent_id = parse_agent_id(&request.params)?;
            commands.resume(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "active" }))
        }
        "agents.kill" => {
            let agent_id = parse_agent_id(&request.params)?;
            commands.kill(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "terminated" }))
        }
        "agents.failover" => {
            let agent_id = parse_agent_id(&request.params)?;
            let agent = commands.failover(agent_id).await?;
            Ok(serde_json::json!({
                "agent_id": agent.agent_id,
                "new_provider": agent.cli_provider,
                "status": agent.status,
            }))
        }
        "taskflow.status" => Ok(serde_json::to_value(commands.taskflow_status()?).unwrap()),
        "taskflow.show" => {
            let m_id = request
                .params
                .as_str()
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing milestone_id in params".to_string(),
                })?;
            Ok(serde_json::to_value(commands.taskflow_show(m_id)?).unwrap())
        }
        "taskflow.assign" => {
            let roadmap_id = request
                .params
                .get("roadmap_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing roadmap_id".to_string(),
                })?;
            let task_id = request
                .params
                .get("task_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing or invalid task_id".to_string(),
                })?;
            commands.taskflow_assign(roadmap_id, task_id)?;
            Ok(serde_json::json!({ "message": "Task assigned" }))
        }
        "taskflow.sync" => Ok(serde_json::to_value(commands.taskflow_sync()?).unwrap()),
        _ => Err(AegisError::IpcProtocol {
            reason: format!("Unknown command: {}", request.command),
        }),
    }
}

fn parse_agent_id(params: &serde_json::Value) -> Result<Uuid> {
    params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(|| AegisError::IpcProtocol {
            reason: "Missing or invalid agent_id".to_string(),
        })
}

async fn handle_subscription(mut lines: Framed<UnixStream, LinesCodec>, event_bus: Arc<EventBus>) {
    let mut rx = event_bus.subscribe();
    while let Ok(event) = rx.recv().await {
        let event_json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => continue,
        };

        if lines.send(event_json).await.is_err() {
            break;
        }
    }
}
