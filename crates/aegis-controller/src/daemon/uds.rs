use crate::daemon::logs::{LogTailer, PaneRelay};
use crate::daemon::projects::ProjectRegistry;
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::{AegisError, Result};
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
            let project_path = request.project_path.as_ref().ok_or_else(|| {
                AegisError::IpcProtocol {
                    reason: "Missing project_path".to_string(),
                }
            })?;
            let project = project_registry.find_by_path(project_path)?.ok_or_else(|| {
                AegisError::ProjectNotInitialized {
                    path: project_path.clone(),
                }
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

            if request.command == "logs.tail" {
                handle_log_tail(lines, &request, runtime.log_tailer.clone()).await;
            } else {
                handle_pane_attach(lines, &request, runtime.pane_relay.clone()).await;
            }
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
    // 1. Handle Global Commands (no project context required)
    match request.command.as_str() {
        "projects.list" => return Ok(serde_json::to_value(project_registry.load()?).unwrap()),
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
        let r = AegisRuntime::load(project.root_path.clone()).await?;
        r.recover().await?;
        runtimes.insert(project.id, r);
        runtimes.get(&project.id).unwrap()
    };

    let commands = runtime.commands();

    match request.command.as_str() {
        "status" => Ok(serde_json::to_value(commands.status()?).unwrap()),
        "agents.list" => Ok(serde_json::to_value(commands.list_agents()?).unwrap()),
        "tasks.list" => Ok(serde_json::to_value(commands.list_tasks()?).unwrap()),
        "channels.list" => Ok(serde_json::to_value(commands.list_channels()?).unwrap()),
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
    tailer: Arc<LogTailer>,
) {
    let agent_id = match parse_agent_id(&request.params) {
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

    let mut log_sink = GenericSink {
        inner: lines,
        kind: "line".to_string(),
    };
    if let Err(e) = tailer.tail(agent_id, last_n, &mut log_sink).await {
        error!("Log tail error for agent {}: {}", agent_id, e);
    }
}

async fn handle_pane_attach(
    lines: Framed<UnixStream, LinesCodec>,
    request: &UdsRequest,
    relay: Arc<PaneRelay>,
) {
    let agent_id = match parse_agent_id(&request.params) {
        Ok(id) => id,
        Err(e) => {
            let mut lines = lines;
            send_error(&mut lines, request.id, e).await;
            return;
        }
    };

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

fn io_error_from_codec(e: tokio_util::codec::LinesCodecError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e)
}
