use crate::daemon::projects::{ProjectRecord, ProjectRegistry};
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::Result;
use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, State, Query, WebSocketUpgrade},
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
            .route("/projects/:id/agents", get(list_agents))
            .route("/projects/:id/commands", post(dispatch_command))
            .route("/ws/events", get(ws_handler))
            .route("/ws/logs/:agent_id", get(ws_logs_handler))
            .route("/ws/pane/:agent_id", get(ws_pane_handler))
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

async fn list_projects(State(state): State<HttpState>) -> Json<Vec<ProjectRecord>> {
    let projects = state.projects.load().unwrap_or_default();
    Json(projects)
}

async fn list_agents(
    Path(id): Path<Uuid>,
    State(_state): State<HttpState>,
) -> Json<serde_json::Value> {
    // STUB: To be implemented with M10 AegisRuntime integration
    Json(serde_json::json!({ "project_id": id, "agents": [] }))
}

async fn dispatch_command(
    Path(id): Path<Uuid>,
    State(_state): State<HttpState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // STUB: To be implemented with M10 AegisRuntime integration
    info!("HTTP Command for project {}: {:?}", id, payload);
    Json(serde_json::json!({ "status": "success", "message": "Command received (stub)" }))
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
                fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_ready(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn start_send(mut self: Pin<&mut Self>, item: String) -> std::result::Result<(), Self::Error> {
                    let msg = MessageWrapper { kind: "line".to_string(), data: item };
                    let json = serde_json::to_string(&msg).unwrap();
                    Pin::new(&mut self.inner).start_send(Message::Text(json.into())).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_flush(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_close(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
            }

            let mut ws_sink = WsSink { inner: sender };
            let _ = runtime.log_tailer.tail(agent_id, last_n, &mut ws_sink).await;
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
                fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_ready(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn start_send(mut self: Pin<&mut Self>, item: Vec<u8>) -> std::result::Result<(), Self::Error> {
                    let msg = MessageWrapper { kind: "output".to_string(), data: BASE64_STANDARD.encode(item) };
                    let json = serde_json::to_string(&msg).unwrap();
                    Pin::new(&mut self.inner).start_send(Message::Text(json.into())).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_flush(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
                }
                fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), Self::Error>> {
                    Pin::new(&mut self.inner).poll_close(cx).map_err(|e| aegis_core::AegisError::Unexpected(Box::new(e)))
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
            let _ = runtime.pane_relay.relay(agent_id, &mut ws_sink, &mut pinned_in_rx).await;
        }
    })
}
