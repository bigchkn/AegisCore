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
use crate::transcript::append_tmux_input;

const INITIAL_PANE_SNAPSHOT_BYTES: usize = 128 * 1024;

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
            out_tx.send(line.to_string()).await?;
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
                    out_tx.send(line.to_string()).await?;
                }

                pos = metadata.len();
            }

            sleep(Duration::from_millis(100)).await;
        }
    }
}

/// An event emitted by `PaneRelay::relay` to describe either a chunk of terminal
/// output or a change in the tmux pane dimensions.
pub enum PaneEvent {
    Output(Vec<u8>),
    Resize { cols: u16, rows: u16 },
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
        mut out_tx: impl Sink<PaneEvent, Error = AegisError> + Unpin,
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

        // 1. Send the current tmux pane dimensions so the web terminal can match exactly.
        if let Ok((cols, rows)) = self.tmux.pane_size(&target).await {
            out_tx.send(PaneEvent::Resize { cols, rows }).await?;
        }

        // 2. Initial burst: replay the recent raw pane log bytes so the
        // browser receives the same ANSI stream format as live updates.
        if let Ok(snapshot) = read_log_tail_bytes(&log_path, INITIAL_PANE_SNAPSHOT_BYTES) {
            if !snapshot.is_empty() {
                out_tx.send(PaneEvent::Output(snapshot)).await?;
            }
        }

        let mut last_size: Option<(u16, u16)> = None;
        let mut size_check_ticks: u32 = 0;

        loop {
            tokio::select! {
                // (a) Read from log file and stream to out_tx; periodically check pane size.
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

                        out_tx.send(PaneEvent::Output(new_bytes)).await?;
                        log_pos = metadata.len();
                    }

                    // Every ~2 s re-query tmux for the pane size.
                    size_check_ticks += 1;
                    if size_check_ticks >= 20 {
                        size_check_ticks = 0;
                        if let Ok(size) = self.tmux.pane_size(&target).await {
                            if last_size != Some(size) {
                                last_size = Some(size);
                                out_tx.send(PaneEvent::Resize { cols: size.0, rows: size.1 }).await?;
                            }
                        }
                    }
                }

                // (b) Receive from in_rx and send to tmux
                Some(input_bytes) = in_rx.next() => {
                    append_tmux_input(&log_path, &input_bytes)?;
                    self.tmux.send_raw_input(&target, &input_bytes).await
                        .map_err(|e| AegisError::IpcConnection { source: std::io::Error::other(e.to_string()) })?;
                }
            }
        }
    }
}

fn read_log_tail_bytes(path: &std::path::Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;

    let mut bytes = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::read_log_tail_bytes;

    #[test]
    fn read_log_tail_bytes_preserves_raw_ansi_bytes() {
        let mut file = NamedTempFile::new().expect("temp file");
        let expected = b"\x1b[38;5;174mhello\x1b[0m\n";
        file.write_all(b"prefix\n").expect("write prefix");
        file.write_all(expected).expect("write ansi");
        file.flush().expect("flush temp file");

        let snapshot = read_log_tail_bytes(file.path(), expected.len()).expect("read log tail");
        assert_eq!(snapshot, expected);
    }
}
