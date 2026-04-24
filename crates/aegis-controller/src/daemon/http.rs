use crate::daemon::projects::{ProjectRecord, ProjectRegistry};
use crate::events::EventBus;
use aegis_core::Result;
use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct HttpState {
    pub event_bus: Arc<EventBus>,
    pub projects: Arc<ProjectRegistry>,
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
            .with_state(state);

        Self { router }
    }

    pub async fn run(self, port: u16) -> Result<()> {
        let addr = format!("127.0.0.1:{}", port);
        info!("HTTP Server listening on {}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
            aegis_core::AegisError::StorageIo {
                path: addr.into(),
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

        if sender.send(Message::Text(event_json)).await.is_err() {
            break;
        }
    }
}
