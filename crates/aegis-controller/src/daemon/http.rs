use crate::daemon::projects::{ProjectRecord, ProjectRegistry};
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::Result;
use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{Sink, SinkExt, StreamExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct HttpState {
    pub event_bus: Arc<EventBus>,
    pub projects: Arc<ProjectRegistry>,
    pub active_runtimes: Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
}

pub struct HttpServer {
    router: Router,
}

impl HttpServer {
    pub fn new(state: HttpState) -> Self {
        let router = Router::new()
            .route("/projects", get(list_projects))
            .route("/projects/:id/status", get(project_status))
            .route("/projects/:id/agents", get(list_agents))
            .route("/projects/:id/tasks", get(list_tasks))
            .route("/projects/:id/channels", get(list_channels))
            .route("/projects/:id/commands", post(dispatch_command))
            .route("/projects/:id/taskflow/status", get(taskflow_status))
            .route(
                "/projects/:id/taskflow/show/:milestone_id",
                get(taskflow_show),
            )
            .route("/ws/events", get(ws_handler))
            .route("/ws/logs/:agent_id", get(ws_logs_handler))
            .route("/ws/pane/:agent_id", get(ws_pane_handler))
            .merge(aegis_web::routes::static_routes())
            .with_state(state);

        Self { router }
    }

    pub async fn run(self, port: u16) -> Result<()> {
        let addr = format!("127.0.0.1:{}", port);
        info!("HTTP Server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
            aegis_core::AegisError::StorageIo {
                path: PathBuf::from(&addr),
                source: e,
            }
        })?;

        axum::serve(listener, self.router)
            .await
            .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))?;

        Ok(())
    }
}

async fn get_runtime(state: &HttpState, project_id: Uuid) -> Result<AegisRuntime> {
    let mut runtimes = state.active_runtimes.lock().await;
    if let Some(r) = runtimes.get(&project_id) {
        return Ok(r.clone());
    }

    let project =
        state
            .projects
            .find_by_id(project_id)?
            .ok_or_else(|| aegis_core::AegisError::Config {
                field: "project_id".to_string(),
                reason: "project not found".to_string(),
            })?;

    let r = AegisRuntime::load(project.root_path.clone()).await?;
    r.recover().await?;
    runtimes.insert(project_id, r.clone());
    Ok(r)
}

async fn list_projects(State(state): State<HttpState>) -> Json<Vec<ProjectRecord>> {
    let projects = state.projects.load().unwrap_or_default();
    Json(projects)
}

async fn project_status(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let status = runtime.commands().status().map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(status).unwrap()))
}

async fn list_agents(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let agents = runtime
        .commands()
        .list_agents()
        .map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(agents).unwrap()))
}

async fn list_tasks(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let tasks = runtime.commands().list_tasks().map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(tasks).unwrap()))
}

async fn list_channels(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let channels = runtime
        .commands()
        .list_channels()
        .map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(channels).unwrap()))
}

async fn dispatch_command(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
    Json(payload): Json<serde_json::Value>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let commands = runtime.commands();

    let cmd = payload
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("Missing command field")?;
    let params = payload.get("params").unwrap_or(&serde_json::Value::Null);

    match cmd {
        "spawn" => {
            let task = params.as_str().ok_or("Missing task string in params")?;
            let task_id = commands.spawn(task).await.map_err(|e| e.to_string())?;
            Ok(Json(serde_json::json!({ "task_id": task_id })))
        }
        "pause" => {
            let agent_id = resolve_agent_id_param(&commands, params, "agent_id")?;
            commands.pause(agent_id).await.map_err(|e| e.to_string())?;
            Ok(Json(serde_json::json!({ "status": "ok" })))
        }
        "resume" => {
            let agent_id = resolve_agent_id_param(&commands, params, "agent_id")?;
            commands.resume(agent_id).await.map_err(|e| e.to_string())?;
            Ok(Json(serde_json::json!({ "status": "ok" })))
        }
        "kill" => {
            let agent_id = resolve_agent_id_param(&commands, params, "agent_id")?;
            commands.kill(agent_id).await.map_err(|e| e.to_string())?;
            Ok(Json(serde_json::json!({ "status": "ok" })))
        }
        "clarify.request" => {
            let agent_raw = params
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or("Missing agent_id")?;
            let question = params
                .get("question")
                .and_then(|v| v.as_str())
                .ok_or("Missing question")?;
            let task_id = params
                .get("task_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok());
            let context = params
                .get("context")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let priority = params.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let request = commands
                .clarify_request(agent_raw, task_id, question, context, priority)
                .map_err(|e| e.to_string())?;
            Ok(Json(serde_json::to_value(request).unwrap()))
        }
        "clarify.list" => {
            if let Some(agent_raw) = params.get("agent_id").and_then(|v| v.as_str()) {
                let requests = commands
                    .clarify_list_for_agent(agent_raw)
                    .map_err(|e| e.to_string())?;
                Ok(Json(serde_json::to_value(requests).unwrap()))
            } else {
                let requests = commands.clarify_list().map_err(|e| e.to_string())?;
                Ok(Json(serde_json::to_value(requests).unwrap()))
            }
        }
        "clarify.show" => {
            let request_id = params
                .get("request_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or("Missing or invalid request_id")?;
            let request = commands
                .clarify_show(request_id)
                .map_err(|e| e.to_string())?;
            Ok(Json(serde_json::to_value(request).unwrap()))
        }
        "clarify.answer" => {
            let request_id = params
                .get("request_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or("Missing or invalid request_id")?;
            let answer = params
                .get("answer")
                .and_then(|v| v.as_str())
                .ok_or("Missing answer")?;
            let payload = params
                .get("payload")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let answered_by = params
                .get("answered_by")
                .and_then(|v| v.as_str())
                .unwrap_or("human_cli");
            let answered_by = match answered_by {
                "human_cli" => crate::clarification::ClarifierSource::HumanCli,
                "human_tui" => crate::clarification::ClarifierSource::HumanTui,
                "telegram" => crate::clarification::ClarifierSource::Telegram,
                "system" => crate::clarification::ClarifierSource::System,
                other => return Err(format!("Unknown clarification source: {other}")),
            };
            let request = commands
                .clarify_answer(request_id, answer, payload, answered_by)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Json(serde_json::to_value(request).unwrap()))
        }
        "clarify.wait" => {
            let target = params
                .get("request_id")
                .or_else(|| params.get("agent_id"))
                .and_then(|v| v.as_str())
                .ok_or("Missing request_id or agent_id")?;
            let timeout = params
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .map(std::time::Duration::from_secs);
            let request = commands
                .clarify_wait(target, timeout)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Json(serde_json::to_value(request).unwrap()))
        }
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

fn resolve_agent_id_param(
    commands: &crate::commands::ControllerCommands,
    params: &serde_json::Value,
    name: &str,
) -> std::result::Result<Uuid, String> {
    let raw = params
        .get(name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing {}", name))?;

    commands.resolve_agent_id(raw).map_err(|e| e.to_string())
}

async fn taskflow_status(
    Path(id): Path<Uuid>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let status = runtime
        .commands()
        .taskflow_status()
        .map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(status).unwrap()))
}

async fn taskflow_show(
    Path((id, milestone_id)): Path<(Uuid, String)>,
    State(state): State<HttpState>,
) -> std::result::Result<Json<serde_json::Value>, String> {
    let runtime = get_runtime(&state, id).await.map_err(|e| e.to_string())?;
    let milestone = runtime
        .commands()
        .taskflow_show(&milestone_id)
        .map_err(|e| e.to_string())?;
    Ok(Json(serde_json::to_value(milestone).unwrap()))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<HttpState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(socket, state.event_bus))
}

async fn handle_ws(socket: WebSocket, event_bus: Arc<EventBus>) {
    let (mut sender, mut _receiver) = socket.split();
    let mut rx = event_bus.subscribe();

    while let Ok(event) = rx.recv().await {
        let event_json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => continue,
        };

        if sender.send(Message::Text(event_json.into())).await.is_err() {
            break;
        }
    }
}

#[derive(serde::Deserialize)]
struct LogQuery {
    last_n: Option<usize>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct MessageWrapper {
    #[serde(rename = "type")]
    kind: String,
    data: String,
}

async fn find_runtime_by_agent(
    active_runtimes: &Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
    agent_id: Uuid,
) -> Option<AegisRuntime> {
    let runtimes = active_runtimes.lock().await;
    for runtime in runtimes.values() {
        if let Ok(Some(_)) = aegis_core::AgentRegistry::get(runtime.registry.as_ref(), agent_id) {
            return Some(runtime.clone());
        }
    }
    None
}

async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<Uuid>,
    Query(query): Query<LogQuery>,
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let runtime = find_runtime_by_agent(&state.active_runtimes, agent_id).await;
    ws.on_upgrade(move |socket| async move {
        if let Some(runtime) = runtime {
            let (sender, _) = socket.split();
            let last_n = query.last_n.unwrap_or(100);

            struct WsSink<S> {
                inner: S,
            }
            impl<S> Sink<String> for WsSink<S>
            where
                S: Sink<Message, Error = axum::Error> + Unpin,
            {
                type Error = aegis_core::AegisError;
                fn poll_ready(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_ready(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn start_send(
                    mut self: Pin<&mut Self>,
                    item: String,
                ) -> std::result::Result<(), Self::Error> {
                    let msg = MessageWrapper {
                        kind: "line".to_string(),
                        data: item,
                    };
                    let json = serde_json::to_string(&msg).unwrap();
                    Pin::new(&mut self.inner)
                        .start_send(Message::Text(json.into()))
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_flush(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_flush(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_close(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_close(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
            }

            let mut ws_sink = WsSink { inner: sender };
            let _ = runtime
                .log_tailer
                .tail(agent_id, last_n, &mut ws_sink)
                .await;
        }
    })
}

async fn ws_pane_handler(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<Uuid>,
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let runtime = find_runtime_by_agent(&state.active_runtimes, agent_id).await;
    ws.on_upgrade(move |socket| async move {
        if let Some(runtime) = runtime {
            use base64::prelude::*;
            let (ws_sender, ws_receiver) = socket.split();

            struct PaneWsSink<S> {
                inner: S,
            }
            impl<S> Sink<Vec<u8>> for PaneWsSink<S>
            where
                S: Sink<Message, Error = axum::Error> + Unpin,
            {
                type Error = aegis_core::AegisError;
                fn poll_ready(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_ready(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn start_send(
                    mut self: Pin<&mut Self>,
                    item: Vec<u8>,
                ) -> std::result::Result<(), Self::Error> {
                    let msg = MessageWrapper {
                        kind: "output".to_string(),
                        data: BASE64_STANDARD.encode(item),
                    };
                    let json = serde_json::to_string(&msg).unwrap();
                    Pin::new(&mut self.inner)
                        .start_send(Message::Text(json.into()))
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_flush(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_flush(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_close(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner)
                        .poll_close(cx)
                        .map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
            }

            let in_rx = ws_receiver.filter_map(|msg_res| async move {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        let msg: MessageWrapper = serde_json::from_str(&text).ok()?;
                        if msg.kind == "input" {
                            BASE64_STANDARD.decode(msg.data).ok()
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            });

            let mut ws_sink = PaneWsSink { inner: ws_sender };
            let mut pinned_in_rx = Box::pin(in_rx);
            let _ = runtime
                .pane_relay
                .relay(agent_id, &mut ws_sink, &mut pinned_in_rx)
                .await;
        }
    })
}
