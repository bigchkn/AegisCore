use crate::commands::ControllerCommands;
use crate::daemon::logs::PaneRelay;
use crate::daemon::projects::ProjectRegistry;
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::{AegisError, AgentKind, Result};
use futures_util::{Sink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
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

        if request.command == "logs.tail" || request.command == "pane.attach" {
            let project_path =
                request
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
                let r = AegisRuntime::load(
                    project.root_path.clone(),
                    Some(Arc::clone(&project_registry)),
                    Some(project.id),
                )
                .await?;
                r.recover().await?;
                r.start().await?;
                runtimes.insert(project.id, r);
                runtimes.get(&project.id).unwrap()
            };

            let commands = runtime.commands();
            if request.command == "logs.tail" {
                handle_log_tail(lines, &request, &commands).await;
            } else {
                handle_pane_attach(
                    lines,
                    &request,
                    &commands,
                    runtime.pane_relay.clone(),
                    &project_registry,
                    project.id,
                )
                .await;
            }
            return Ok(());
        }

        // Logic to find project and dispatch command
        let payload = match dispatch_command(&request, Arc::clone(&project_registry), &active_runtimes).await {
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
    project_registry: Arc<ProjectRegistry>,
    active_runtimes: &Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
) -> Result<serde_json::Value> {
    // 1. Handle Global Commands (no project context required)
    match request.command.as_str() {
        "daemon.status" => {
            let projects = project_registry.load()?.len();
            return Ok(serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_s": 0, // TODO: Implement actual uptime
                "projects": projects,
                "socket_path": "/tmp/aegis.sock", // TODO: Pass this from supervisor
            }));
        }
        "projects.list" => {
            let mut projects = project_registry.load()?;
            let runtimes = active_runtimes.lock().await;
            for p in &mut projects {
                if runtimes.contains_key(&p.id) {
                    p.status = Some("active".to_string());
                } else {
                    p.status = Some("idle".to_string());
                }
            }
            return Ok(serde_json::to_value(projects).unwrap());
        }
        "projects.register" => {
            let root_path = request
                .params
                .get("root_path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing root_path in params".to_string(),
                })?;
            let project = project_registry.register(root_path)?;
            return Ok(serde_json::to_value(project).unwrap());
        }
        _ => {}
    }

    // 2. Handle Project-Specific Commands
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
        let r = AegisRuntime::load(
            project.root_path.clone(),
            Some(Arc::clone(&project_registry)),
            Some(project.id),
        )
        .await?;
        r.recover().await?;
        runtimes.insert(project.id, r);
        runtimes.get(&project.id).unwrap()
    };

    let commands = runtime.commands();

    match request.command.as_str() {
        "session.start" => {
            let role = request.params.get("role").and_then(|v| v.as_str());
            let active_agents = commands.list_agents()?;

            if active_agents.is_empty() {
                runtime.start().await?;
            } else if let Some(role) = role {
                let already_running = active_agents
                    .iter()
                    .any(|agent| agent.kind == AgentKind::Bastion && agent.name == role);
                if !already_running {
                    let agent = runtime.dispatcher.spawn_bastion(role).await?;
                    return Ok(serde_json::json!([agent]));
                }
            }

            let agents = commands.list_agents()?;
            let bastions: Vec<_> = agents
                .into_iter()
                .filter(|agent| agent.kind == AgentKind::Bastion)
                .collect();
            Ok(serde_json::to_value(bastions).unwrap())
        }
        "session.stop" => {
            let force = request
                .params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let agents = commands.list_agents()?;

            for agent in agents {
                if force {
                    commands.kill(agent.agent_id).await?;
                } else {
                    commands.pause(agent.agent_id).await?;
                }
            }

            runtime.shutdown().await?;

            Ok(serde_json::json!({
                "force": force,
                "message": if force {
                    "All agents terminated."
                } else {
                    "Session stopped. Agent worktrees preserved."
                }
            }))
        }
        "status" | "project.status" => {
            use aegis_core::{AgentStatus, TaskStatus};
            let agents = commands.list_agents()?;
            let tasks = commands.list_tasks()?;
            let active_agents = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Active)
                .count() as u64;
            let queued_agents = agents
                .iter()
                .filter(|a| a.status == AgentStatus::Queued)
                .count() as u64;
            let providers: Vec<String> = runtime.config.providers.keys().cloned().collect();
            Ok(serde_json::json!({
                "project_root": runtime.root_path.display().to_string(),
                "session_name": runtime.config.global.tmux_session_name,
                "agents": {
                    "active": active_agents,
                    "queued": queued_agents,
                    "total": agents.len() as u64,
                },
                "tasks": {
                    "active": tasks.iter().filter(|t| t.status == TaskStatus::Active).count() as u64,
                    "complete": tasks.iter().filter(|t| t.status == TaskStatus::Complete).count() as u64,
                    "failed": tasks.iter().filter(|t| t.status == TaskStatus::Failed).count() as u64,
                },
                "watchdog": {
                    "interval_ms": runtime.config.watchdog.poll_interval_ms,
                },
                "providers": providers,
                "last_attached_agent_id": project.last_attached_agent_id,
            }))
        }
        "agents.list" => Ok(serde_json::to_value(commands.list_agents()?).unwrap()),
        "tasks.list" => Ok(serde_json::to_value(commands.list_tasks()?).unwrap()),
        "channels.list" => Ok(serde_json::to_value(commands.list_channels()?).unwrap()),
        "message.send" => {
            let to_agent_raw = request
                .params
                .get("to_agent_id")
                .or_else(|| request.params.get("agent_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing to_agent_id".to_string(),
                })?;
            let from_agent_id = request
                .params
                .get("from_agent_id")
                .and_then(|v| v.as_str())
                .map(parse_uuid)
                .transpose()?;
            let kind = parse_message_type(request.params.get("kind"))?;
            let payload = request
                .params
                .get("payload")
                .or_else(|| request.params.get("message"))
                .cloned()
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing payload".to_string(),
                })?;
            let receipt = commands
                .send_message(from_agent_id, to_agent_raw, kind, payload)
                .await?;
            Ok(serde_json::to_value(receipt).unwrap())
        }
        "message.inbox" => {
            let agent_raw = request
                .params
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing agent_id".to_string(),
                })?;
            Ok(serde_json::to_value(commands.inbox(agent_raw)?).unwrap())
        }
        "message.list" => {
            if let Some(agent_raw) = request.params.get("agent_id").and_then(|v| v.as_str()) {
                Ok(serde_json::to_value(commands.inbox(agent_raw)?).unwrap())
            } else {
                Ok(serde_json::to_value(commands.list_inboxes()?).unwrap())
            }
        }
        "clarify.request" => {
            let agent_raw = request
                .params
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing agent_id".to_string(),
                })?;
            let question = request
                .params
                .get("question")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing question".to_string(),
                })?;
            let task_id = request
                .params
                .get("task_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok());
            let context = request
                .params
                .get("context")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let priority = request
                .params
                .get("priority")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let clar = commands.clarify_request(agent_raw, task_id, question, context, priority)?;
            Ok(serde_json::to_value(clar).unwrap())
        }
        "clarify.list" => {
            if let Some(agent_raw) = request.params.get("agent_id").and_then(|v| v.as_str()) {
                Ok(serde_json::to_value(commands.clarify_list_for_agent(agent_raw)?).unwrap())
            } else {
                Ok(serde_json::to_value(commands.clarify_list()?).unwrap())
            }
        }
        "clarify.show" => {
            let request_id = request
                .params
                .get("request_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing or invalid request_id".to_string(),
                })?;
            Ok(serde_json::to_value(commands.clarify_show(request_id)?).unwrap())
        }
        "clarify.answer" => {
            let request_id = request
                .params
                .get("request_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing or invalid request_id".to_string(),
                })?;
            let answer = request
                .params
                .get("answer")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing answer".to_string(),
                })?;
            let payload = request
                .params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let answered_by = request
                .params
                .get("answered_by")
                .and_then(|v| v.as_str())
                .unwrap_or("human_cli");
            let answered_by = parse_clarifier_source(answered_by)?;
            let clar = commands
                .clarify_answer(request_id, answer, payload, answered_by)
                .await?;
            Ok(serde_json::to_value(clar).unwrap())
        }
        "clarify.wait" => {
            let target = request
                .params
                .get("request_id")
                .or_else(|| request.params.get("agent_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing request_id or agent_id".to_string(),
                })?;
            let timeout = request
                .params
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .map(std::time::Duration::from_secs);
            let clar = commands.clarify_wait(target, timeout).await?;
            Ok(serde_json::to_value(clar).unwrap())
        }
        "agents.spawn" => {
            let task = request
                .params
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing task".to_string(),
                })?;
            let task_id = commands.spawn(task).await?;
            Ok(serde_json::json!({ "task_id": task_id }))
        }
        "design.spawn" => {
            let rendered: aegis_design::RenderedTemplate =
                serde_json::from_value(request.params.clone()).map_err(|e| {
                    AegisError::IpcProtocol {
                        reason: format!("design.spawn: invalid RenderedTemplate: {e}"),
                    }
                })?;
            let agent = commands.spawn_from_template(rendered).await?;
            Ok(serde_json::json!({
                "agent_id": agent.agent_id,
                "role": agent.role,
                "kind": format!("{:?}", agent.kind),
            }))
        }
        "agents.pause" => {
            let agent_id = parse_agent_id(&request.params, &commands)?;
            commands.pause(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "paused" }))
        }
        "agents.resume" => {
            let agent_id = parse_agent_id(&request.params, &commands)?;
            commands.resume(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "active" }))
        }
        "agents.kill" => {
            let agent_id = parse_agent_id(&request.params, &commands)?;
            commands.kill(agent_id).await?;
            Ok(serde_json::json!({ "agent_id": agent_id, "status": "terminated" }))
        }
        "agents.failover" => {
            let agent_id = parse_agent_id(&request.params, &commands)?;
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
        "taskflow.create_milestone" => {
            let id = request
                .params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing id".to_string(),
                })?;
            let name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing name".to_string(),
                })?;
            let lld = request.params.get("lld").and_then(|v| v.as_str());
            commands.taskflow_create_milestone(id, name, lld)?;
            Ok(serde_json::json!({ "message": "Milestone created" }))
        }
        "taskflow.add_task" => {
            let milestone_id = request
                .params
                .get("milestone_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing milestone_id".to_string(),
                })?;
            let id = request
                .params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing id".to_string(),
                })?;
            let task = request
                .params
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing task".to_string(),
                })?;
            let task_type: aegis_taskflow::model::TaskType = request
                .params
                .get("task_type")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            commands.taskflow_add_task(milestone_id, id, task, task_type)?;
            Ok(serde_json::json!({ "message": "Task added" }))
        }
        "taskflow.set_task_status" => {
            let milestone_id = request
                .params
                .get("milestone_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing milestone_id".to_string(),
                })?;
            let task_id = request
                .params
                .get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing task_id".to_string(),
                })?;
            let status = request
                .params
                .get("status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AegisError::IpcProtocol {
                    reason: "Missing status".to_string(),
                })?;
            commands.taskflow_set_task_status(milestone_id, task_id, status)?;
            Ok(serde_json::json!({ "message": "Task status updated" }))
        }
        "taskflow.next" => {
            Ok(serde_json::to_value(commands.taskflow_next()?).unwrap())
        }
        _ => Err(AegisError::IpcProtocol {
            reason: format!("Unknown command: {}", request.command),
        }),
    }
}

fn parse_agent_id(params: &serde_json::Value, commands: &ControllerCommands) -> Result<Uuid> {
    let raw = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AegisError::IpcProtocol {
            reason: "Missing agent_id".to_string(),
        })?;

    commands
        .resolve_agent_id(raw)
        .map_err(|e| AegisError::IpcProtocol {
            reason: e.to_string(),
        })
}

fn parse_uuid(raw: &str) -> Result<Uuid> {
    Uuid::parse_str(raw).map_err(|_| AegisError::IpcProtocol {
        reason: format!("Missing or invalid UUID `{raw}`"),
    })
}

fn parse_message_type(value: Option<&serde_json::Value>) -> Result<aegis_core::MessageType> {
    let Some(value) = value else {
        return Ok(aegis_core::MessageType::Notification);
    };

    let raw = value.as_str().ok_or_else(|| AegisError::IpcProtocol {
        reason: "kind must be a string".to_string(),
    })?;

    serde_json::from_str::<aegis_core::MessageType>(&format!("{raw:?}")).map_err(|e| {
        AegisError::IpcProtocol {
            reason: format!("Invalid message kind `{raw}`: {e}"),
        }
    })
}

fn parse_clarifier_source(value: &str) -> Result<crate::clarification::ClarifierSource> {
    match value {
        "human_cli" => Ok(crate::clarification::ClarifierSource::HumanCli),
        "human_tui" => Ok(crate::clarification::ClarifierSource::HumanTui),
        "telegram" => Ok(crate::clarification::ClarifierSource::Telegram),
        "system" => Ok(crate::clarification::ClarifierSource::System),
        other => Err(AegisError::IpcProtocol {
            reason: format!("Unknown clarification source `{other}`"),
        }),
    }
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

#[derive(Debug, Serialize, Deserialize)]
struct MessageWrapper {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}

struct GenericSink<S> {
    inner: S,
    kind: String,
}

impl<S> Sink<String> for GenericSink<S>
where
    S: Sink<String, Error = tokio_util::codec::LinesCodecError> + Unpin,
{
    type Error = AegisError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_ready(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn start_send(mut self: Pin<&mut Self>, item: String) -> Result<()> {
        let msg = MessageWrapper {
            kind: self.kind.clone(),
            data: item,
        };
        let json = serde_json::to_string(&msg).unwrap();
        Pin::new(&mut self.inner)
            .start_send(json)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_flush(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_close(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }
}

impl<S> Sink<Vec<u8>> for GenericSink<S>
where
    S: Sink<String, Error = tokio_util::codec::LinesCodecError> + Unpin,
{
    type Error = AegisError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_ready(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> Result<()> {
        use base64::prelude::*;
        let msg = MessageWrapper {
            kind: self.kind.clone(),
            data: BASE64_STANDARD.encode(item),
        };
        let json = serde_json::to_string(&msg).unwrap();
        Pin::new(&mut self.inner)
            .start_send(json)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_flush(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.inner)
            .poll_close(cx)
            .map_err(|e| AegisError::IpcConnection {
                source: io_error_from_codec(e),
            })
    }
}

async fn handle_log_tail(
    mut lines: Framed<UnixStream, LinesCodec>,
    request: &UdsRequest,
    commands: &ControllerCommands,
) {
    let agent_id = match parse_agent_id(&request.params, commands) {
        Ok(id) => id,
        Err(e) => {
            send_error(&mut lines, request.id, e).await;
            return;
        }
    };

    let last_n = request
        .params
        .get("last_n")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    match commands.logs(agent_id, Some(last_n)) {
        Ok(logs) => {
            send_response(
                &mut lines,
                request.id,
                serde_json::to_value(logs).unwrap_or(serde_json::Value::Null),
            )
            .await
        }
        Err(e) => send_error(&mut lines, request.id, e).await,
    }
}

async fn handle_pane_attach(
    lines: Framed<UnixStream, LinesCodec>,
    request: &UdsRequest,
    commands: &ControllerCommands,
    relay: Arc<PaneRelay>,
    project_registry: &ProjectRegistry,
    project_id: Uuid,
) {
    let agent_id = match parse_agent_id(&request.params, commands) {
        Ok(id) => id,
        Err(e) => {
            let mut lines = lines;
            send_error(&mut lines, request.id, e).await;
            return;
        }
    };

    // Persist attachment target
    if let Err(e) = project_registry.update_last_attached(project_id, Some(agent_id)) {
        tracing::error!("Failed to persist last_attached_agent_id: {}", e);
    }

    use base64::prelude::*;
    let (uds_sink, uds_stream) = lines.split();

    let in_rx = uds_stream.filter_map(|line_res| async move {
        match line_res {
            Ok(line) => {
                let msg: MessageWrapper = serde_json::from_str(&line).ok()?;
                if msg.kind == "input" {
                    BASE64_STANDARD.decode(msg.data).ok()
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    });

    let mut pane_sink = GenericSink {
        inner: uds_sink,
        kind: "output".to_string(),
    };
    let mut pinned_in_rx = Box::pin(in_rx);

    if let Err(e) = relay
        .relay(agent_id, &mut pane_sink, &mut pinned_in_rx)
        .await
    {
        error!("Pane relay error for agent {}: {}", agent_id, e);
    }
}

async fn send_error(lines: &mut Framed<UnixStream, LinesCodec>, id: Uuid, error: AegisError) {
    let response = UdsResponse {
        id,
        status: "error".to_string(),
        payload: serde_json::Value::Null,
        error: Some(error.to_string()),
    };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = lines.send(json).await;
    }
}

async fn send_response(
    lines: &mut Framed<UnixStream, LinesCodec>,
    id: Uuid,
    payload: serde_json::Value,
) {
    let response = build_success_response(id, payload);
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = lines.send(json).await;
    }
}

fn build_success_response(id: Uuid, payload: serde_json::Value) -> UdsResponse {
    UdsResponse {
        id,
        status: "success".to_string(),
        payload,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::projects::ProjectRegistry;
    use crate::runtime::AegisRuntime;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;

    #[test]
    fn build_success_response_preserves_id_and_payload() {
        let id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let payload = serde_json::json!(["line one", "line two"]);

        let response = build_success_response(id, payload.clone());

        assert_eq!(response.id, id);
        assert_eq!(response.status, "success");
        assert_eq!(response.payload, payload);
        assert!(response.error.is_none());

        let json = serde_json::to_value(response).unwrap();
        assert_eq!(
            json.get("id").and_then(|v| v.as_str()),
            Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        );
        assert_eq!(
            json.get("payload"),
            Some(&serde_json::json!(["line one", "line two"]))
        );
    }

    fn home_lock() -> &'static Mutex<()> {
        static HOME_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        HOME_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_minimal_config(project_root: &std::path::Path) {
        let config = r#"
[providers.claude-code]
binary = "claude-code"

[splinter_defaults]
cli_provider = "claude-code"
"#;
        fs::write(project_root.join("aegis.toml"), config).unwrap();
    }

    fn request(
        command: &str,
        project_path: Option<&std::path::Path>,
        params: serde_json::Value,
    ) -> UdsRequest {
        UdsRequest {
            id: Uuid::new_v4(),
            project_path: project_path.map(|p| p.to_path_buf()),
            command: command.to_string(),
            params,
        }
    }

    #[tokio::test]
    async fn known_ipc_commands_do_not_fall_through_to_unknown_command() {
        let _guard = home_lock().lock().unwrap();

        let home = tempdir().unwrap();
        std::env::set_var("HOME", home.path());

        let project = tempdir().unwrap();
        write_minimal_config(project.path());

        let registry = ProjectRegistry::new();
        registry.register(project.path().to_path_buf()).unwrap();

        let runtimes: Arc<AsyncMutex<HashMap<Uuid, AegisRuntime>>> =
            Arc::new(AsyncMutex::new(HashMap::new()));
        let project_path = project.path();

        let cases = vec![
            request("daemon.status", None, serde_json::Value::Null),
            request("projects.list", None, serde_json::Value::Null),
            request(
                "projects.register",
                None,
                serde_json::json!({ "root_path": project_path.display().to_string() }),
            ),
            request(
                "project.status",
                Some(project_path),
                serde_json::Value::Null,
            ),
            request("agents.list", Some(project_path), serde_json::Value::Null),
            request("tasks.list", Some(project_path), serde_json::Value::Null),
            request("channels.list", Some(project_path), serde_json::Value::Null),
            request("session.start", Some(project_path), serde_json::Value::Null),
            request(
                "session.stop",
                Some(project_path),
                serde_json::json!({ "force": true }),
            ),
            request(
                "agents.pause",
                Some(project_path),
                serde_json::json!({ "agent_id": Uuid::new_v4() }),
            ),
            request(
                "agents.resume",
                Some(project_path),
                serde_json::json!({ "agent_id": Uuid::new_v4() }),
            ),
            request(
                "agents.kill",
                Some(project_path),
                serde_json::json!({ "agent_id": Uuid::new_v4() }),
            ),
            request(
                "agents.failover",
                Some(project_path),
                serde_json::json!({ "agent_id": Uuid::new_v4() }),
            ),
            request(
                "taskflow.status",
                Some(project_path),
                serde_json::Value::Null,
            ),
            request("taskflow.show", Some(project_path), serde_json::json!("M1")),
            request(
                "taskflow.assign",
                Some(project_path),
                serde_json::json!({ "roadmap_id": "M1", "task_id": Uuid::new_v4() }),
            ),
            request(
                "message.send",
                Some(project_path),
                serde_json::json!({
                    "to_agent_id": Uuid::new_v4(),
                    "message": "hello",
                    "kind": "notification"
                }),
            ),
            request(
                "message.inbox",
                Some(project_path),
                serde_json::json!({ "agent_id": Uuid::new_v4() }),
            ),
            request("message.list", Some(project_path), serde_json::Value::Null),
            request(
                "clarify.request",
                Some(project_path),
                serde_json::Value::Null,
            ),
            request("clarify.list", Some(project_path), serde_json::Value::Null),
            request("clarify.show", Some(project_path), serde_json::json!({})),
            request(
                "clarify.answer",
                Some(project_path),
                serde_json::Value::Null,
            ),
            request("clarify.wait", Some(project_path), serde_json::Value::Null),
            request(
                "design.spawn",
                Some(project_path),
                serde_json::Value::Null,
            ),
            request(
                "taskflow.next",
                Some(project_path),
                serde_json::Value::Null,
            ),
        ];

        for case in cases {
            let result = dispatch_command(&case, &registry, &runtimes).await;
            if let Err(err) = &result {
                assert!(
                    !err.to_string().contains("Unknown command"),
                    "command `{}` unexpectedly fell through: {}",
                    case.command,
                    err
                );
            }
        }
    }
}

fn io_error_from_codec(e: tokio_util::codec::LinesCodecError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}
