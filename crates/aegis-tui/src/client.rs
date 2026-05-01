use std::path::PathBuf;

use aegis_core::AegisEvent;
use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::{Framed, LinesCodec};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectRecord {
    pub id: Uuid,
    pub root_path: PathBuf,
    pub auto_start: bool,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub last_attached_agent_id: Option<Uuid>,
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageWrapper {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClarificationRequest {
    pub request_id: Uuid,
    pub agent_id: Uuid,
    pub task_id: Option<Uuid>,
    pub question: String,
    pub context: serde_json::Value,
    pub priority: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct AegisClient {
    uds_path: PathBuf,
    project_path: PathBuf,
}

impl AegisClient {
    pub fn new(uds_path: PathBuf, project_path: PathBuf) -> Self {
        Self {
            uds_path,
            project_path,
        }
    }

    async fn connect(&self) -> anyhow::Result<Framed<UnixStream, LinesCodec>> {
        let stream = UnixStream::connect(&self.uds_path)
            .await
            .context("Failed to connect to aegisd")?;
        Ok(Framed::new(stream, LinesCodec::new()))
    }

    pub async fn send_command(
        &self,
        command: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let mut framed = self.connect().await?;
        let id = Uuid::new_v4();
        let request = UdsRequest {
            id,
            project_path: Some(self.project_path.clone()),
            command: command.to_string(),
            params,
        };

        let json = serde_json::to_string(&request)?;
        framed.send(json).await?;

        while let Some(line_res) = framed.next().await {
            let line = line_res?;
            let response: UdsResponse = serde_json::from_str(&line)?;
            if response.id == id {
                if response.status == "success" {
                    return Ok(response.payload);
                } else {
                    return Err(anyhow!(response
                        .error
                        .unwrap_or_else(|| "Unknown error".to_string())));
                }
            }
        }

        Err(anyhow!("No response from daemon"))
    }

    pub async fn subscribe(&self) -> anyhow::Result<mpsc::Receiver<AegisEvent>> {
        let mut framed = self.connect().await?;
        let request = UdsRequest {
            id: Uuid::new_v4(),
            project_path: None,
            command: "subscribe".to_string(),
            params: serde_json::Value::Null,
        };

        let json = serde_json::to_string(&request)?;
        framed.send(json).await?;

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(line_res) = framed.next().await {
                if let Ok(line) = line_res {
                    if let Ok(event) = serde_json::from_str::<AegisEvent>(&line) {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn tail_logs(
        &self,
        agent_id: Uuid,
        last_n: usize,
    ) -> anyhow::Result<mpsc::Receiver<String>> {
        let mut framed = self.connect().await?;
        let request = UdsRequest {
            id: Uuid::new_v4(),
            project_path: Some(self.project_path.clone()),
            command: "logs.tail".to_string(),
            params: serde_json::json!({ "agent_id": agent_id, "last_n": last_n }),
        };

        let json = serde_json::to_string(&request)?;
        framed.send(json).await?;

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            while let Some(line_res) = framed.next().await {
                if let Ok(line) = line_res {
                    if let Ok(msg) = serde_json::from_str::<MessageWrapper>(&line) {
                        if msg.kind == "line" && tx.send(msg.data).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn attach_pane(
        &self,
        agent_id: Uuid,
    ) -> anyhow::Result<(mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>)> {
        let mut framed = self.connect().await?;
        let request = UdsRequest {
            id: Uuid::new_v4(),
            project_path: Some(self.project_path.clone()),
            command: "pane.attach".to_string(),
            params: serde_json::json!({ "agent_id": agent_id }),
        };

        let json = serde_json::to_string(&request)?;
        framed.send(json).await?;

        let (in_tx, mut in_rx) = mpsc::channel::<Vec<u8>>(100);
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(100);

        use base64::prelude::*;

        tokio::spawn(async move {
            let (mut sink, mut stream) = framed.split();

            tokio::select! {
                // From Daemon to TUI
                _ = async {
                    while let Some(line_res) = stream.next().await {
                        if let Ok(line) = line_res {
                            if let Ok(msg) = serde_json::from_str::<MessageWrapper>(&line) {
                                if msg.kind == "output" {
                                    if let Ok(bytes) = BASE64_STANDARD.decode(msg.data) {
                                        if out_tx.send(bytes).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                } => {}

                // From TUI to Daemon
                _ = async {
                    while let Some(bytes) = in_rx.recv().await {
                        let msg = MessageWrapper {
                            kind: "input".to_string(),
                            data: BASE64_STANDARD.encode(bytes),
                        };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sink.send(json).await.is_err() {
                                break;
                            }
                        }
                    }
                } => {}
            }
        });

        Ok((in_tx, out_rx))
    }

    pub async fn clarify_list(
        &self,
        agent_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<ClarificationRequest>> {
        let params = if let Some(id) = agent_id {
            serde_json::json!({ "agent_id": id })
        } else {
            serde_json::Value::Null
        };

        let payload = self.send_command("clarify.list", params).await?;
        let requests: Vec<ClarificationRequest> = serde_json::from_value(payload)?;
        Ok(requests)
    }

    pub async fn clarify_answer(
        &self,
        request_id: Uuid,
        answer: String,
        payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "request_id": request_id,
            "answer": answer,
            "payload": payload,
            "answered_by": "human_tui"
        });

        self.send_command("clarify.answer", params).await?;
        Ok(())
    }
}
