use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::Duration;

use aegis_core::{AegisError, AgentRegistry, Result, StorageBackend};
use aegis_tmux::{TmuxClient, TmuxTarget};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use tokio::time::sleep;
use uuid::Uuid;

use crate::registry::FileRegistry;
use crate::storage::ProjectStorage;

pub struct LogTailer {
    storage: Arc<ProjectStorage>,
}

impl LogTailer {
    pub fn new(storage: Arc<ProjectStorage>) -> Self {
        Self { storage }
    }

    pub async fn tail(
        &self,
        agent_id: Uuid,
        last_n: usize,
        mut out_tx: impl Sink<String, Error = AegisError> + Unpin,
    ) -> Result<()> {
        let log_path = self.storage.agent_log_path(agent_id);

        let mut file = std::fs::File::open(&log_path).map_err(|_| AegisError::LogFileNotFound {
            agent_id,
            path: log_path.clone(),
        })?;

        // 1. Initial burst: last_n lines
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|source| AegisError::StorageIo {
                path: log_path.clone(),
                source,
            })?;

        let text = String::from_utf8_lossy(&buffer);
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(last_n);

        for line in &lines[start..] {
            let stripped = strip_ansi_escapes::strip(line);
            let line_clean = String::from_utf8_lossy(&stripped).into_owned();
            out_tx.send(line_clean).await?;
        }

        // 2. Continuous tail
        let mut pos = file
            .seek(SeekFrom::End(0))
            .map_err(|source| AegisError::StorageIo {
                path: log_path.clone(),
                source,
            })?;

        loop {
            let metadata =
                std::fs::metadata(&log_path).map_err(|source| AegisError::StorageIo {
                    path: log_path.clone(),
                    source,
                })?;

            if metadata.len() > pos {
                file.seek(SeekFrom::Start(pos))
                    .map_err(|source| AegisError::StorageIo {
                        path: log_path.clone(),
                        source,
                    })?;

                let mut new_bytes = Vec::new();
                file.read_to_end(&mut new_bytes)
                    .map_err(|source| AegisError::StorageIo {
                        path: log_path.clone(),
                        source,
                    })?;

                let new_text = String::from_utf8_lossy(&new_bytes);
                for line in new_text.lines() {
                    let stripped = strip_ansi_escapes::strip(line);
                    let line_clean = String::from_utf8_lossy(&stripped).into_owned();
                    out_tx.send(line_clean).await?;
                }

                pos = metadata.len();
            }

            sleep(Duration::from_millis(100)).await;
        }
    }
}

pub struct PaneRelay {
    storage: Arc<ProjectStorage>,
    registry: Arc<FileRegistry>,
    tmux: Arc<TmuxClient>,
}

impl PaneRelay {
    pub fn new(
        storage: Arc<ProjectStorage>,
        registry: Arc<FileRegistry>,
        tmux: Arc<TmuxClient>,
    ) -> Self {
        Self {
            storage,
            registry,
            tmux,
        }
    }

    pub async fn relay(
        &self,
        agent_id: Uuid,
        mut out_tx: impl Sink<Vec<u8>, Error = AegisError> + Unpin,
        mut in_rx: impl Stream<Item = Vec<u8>> + Unpin,
    ) -> Result<()> {
        let agent = AgentRegistry::get(self.registry.as_ref(), agent_id)?
            .ok_or(AegisError::AgentNotFound { agent_id })?;

        let target =
            TmuxTarget::parse(&agent.tmux_target()).map_err(|e| AegisError::IpcProtocol {
                reason: e.to_string(),
            })?;

        let log_path = self.storage.agent_log_path(agent_id);
        let mut log_file =
            std::fs::File::open(&log_path).map_err(|_| AegisError::LogFileNotFound {
                agent_id,
                path: log_path.clone(),
            })?;

        // Start tailing from current end for the relay
        let mut log_pos =
            log_file
                .seek(SeekFrom::End(0))
                .map_err(|source| AegisError::StorageIo {
                    path: log_path.clone(),
                    source,
                })?;

        loop {
            tokio::select! {
                // (a) Read from log file and stream to out_tx
                _ = sleep(Duration::from_millis(100)) => {
                    let metadata = std::fs::metadata(&log_path).map_err(|source| AegisError::StorageIo {
                        path: log_path.clone(),
                        source,
                    })?;

                    if metadata.len() > log_pos {
                        log_file.seek(SeekFrom::Start(log_pos)).map_err(|source| AegisError::StorageIo {
                            path: log_path.clone(),
                            source,
                        })?;

                        let mut new_bytes = Vec::new();
                        log_file.read_to_end(&mut new_bytes).map_err(|source| AegisError::StorageIo {
                            path: log_path.clone(),
                            source,
                        })?;

                        out_tx.send(new_bytes).await?;
                        log_pos = metadata.len();
                    }
                }

                // (b) Receive from in_rx and send to tmux
                Some(input_bytes) = in_rx.next() => {
                    self.tmux.send_raw_input(&target, &input_bytes).await
                        .map_err(|e| AegisError::IpcConnection { source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()) })?;
                }
            }
        }
    }
}
