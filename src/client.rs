use std::path::{Path, PathBuf};
use aegis_controller::daemon::uds::{UdsRequest, UdsResponse};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};
use uuid::Uuid;
use futures_util::SinkExt;
use crate::error::AegisCliError;

pub struct DaemonClient {
    uds_path: PathBuf,
}

impl DaemonClient {
    pub fn new(path: PathBuf) -> Self {
        Self { uds_path: path }
    }

    pub fn uds_path(&self) -> &Path {
        &self.uds_path
    }

    async fn connect(&self) -> Result<Framed<UnixStream, LinesCodec>, AegisCliError> {
        UnixStream::connect(&self.uds_path)
            .await
            .map(|s| Framed::new(s, LinesCodec::new()))
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::ConnectionRefused
                    || e.kind() == std::io::ErrorKind::NotFound
                {
                    AegisCliError::DaemonNotRunning
                } else {
                    AegisCliError::Io(e)
                }
            })
    }

    pub async fn request(
        &self,
        project_path: Option<&Path>,
        command: &str,
        params: Value,
    ) -> Result<Value, AegisCliError> {
        let mut framed = self.connect().await?;

        let req = UdsRequest {
            id: Uuid::new_v4(),
            project_path: project_path.map(PathBuf::from),
            command: command.to_string(),
            params,
        };
        let json = serde_json::to_string(&req)
            .map_err(|e| AegisCliError::Core(aegis_core::AegisError::IpcProtocol { reason: e.to_string() }))?;

        framed
            .send(json)
            .await
            .map_err(|e| AegisCliError::Core(aegis_core::AegisError::IpcProtocol { reason: e.to_string() }))?;

        match framed.next().await {
            Some(Ok(line)) => {
                let resp: UdsResponse = serde_json::from_str(&line)
                    .map_err(|e| AegisCliError::Core(aegis_core::AegisError::IpcProtocol { reason: e.to_string() }))?;
                if resp.status == "success" {
                    Ok(resp.payload)
                } else {
                    Err(AegisCliError::DaemonError(
                        resp.error.unwrap_or_else(|| "unknown error".into()),
                    ))
                }
            }
            Some(Err(e)) => Err(AegisCliError::Core(
                aegis_core::AegisError::IpcProtocol { reason: e.to_string() },
            )),
            None => Err(AegisCliError::Core(
                aegis_core::AegisError::IpcProtocol { reason: "No response from daemon".into() },
            )),
        }
    }

    /// Subscribe to the daemon event stream. Calls the given closure for each line received.
    pub async fn subscribe_lines<F>(&self, mut on_line: F) -> Result<(), AegisCliError>
    where
        F: FnMut(String) -> bool, // return false to stop
    {
        let mut framed = self.connect().await?;

        let req = UdsRequest {
            id: Uuid::new_v4(),
            project_path: None,
            command: "subscribe".to_string(),
            params: serde_json::json!({}),
        };
        let json = serde_json::to_string(&req)
            .map_err(|e| AegisCliError::Core(aegis_core::AegisError::IpcProtocol { reason: e.to_string() }))?;
        framed
            .send(json)
            .await
            .map_err(|e| AegisCliError::Core(aegis_core::AegisError::IpcProtocol { reason: e.to_string() }))?;

        while let Some(result) = framed.next().await {
            match result {
                Ok(line) => {
                    if !on_line(line) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        Ok(())
    }

    /// Check whether the daemon socket is reachable.
    pub async fn is_reachable(&self) -> bool {
        self.connect().await.is_ok()
    }
}
