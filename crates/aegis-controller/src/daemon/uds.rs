use crate::events::EventBus;
use aegis_core::{AegisError, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
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
}

impl UdsServer {
    pub async fn bind(path: PathBuf, event_bus: Arc<EventBus>) -> Result<Self> {
        // Clean up existing socket if any
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }

        let listener =
            UnixListener::bind(path).map_err(|e| AegisError::IpcConnection { source: e })?;

        Ok(Self {
            listener,
            event_bus,
        })
    }

    pub async fn run(self) {
        info!("UDS Server listening for connections (Unix Domain Socket)");
        loop {
            match self.listener.accept().await {
                Ok((stream, _addr)) => {
                    let bus = Arc::clone(&self.event_bus);
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, bus).await {
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

async fn handle_connection(stream: UnixStream, event_bus: Arc<EventBus>) -> Result<()> {
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
            // Special handling for event subscription
            handle_subscription(lines, event_bus).await;
            return Ok(());
        }

        let response = UdsResponse {
            id: request.id,
            status: "success".to_string(),
            payload: serde_json::json!({ "message": "Command received (stub)" }),
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
