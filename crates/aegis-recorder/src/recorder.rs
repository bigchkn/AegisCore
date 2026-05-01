use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
};

use aegis_core::{
    config::RecorderConfig, AegisError, Agent, LogQuery, Recorder, Result, StorageBackend,
};
use aegis_tmux::{TmuxClient, TmuxTarget};
use chrono::Utc;
use tracing::info;
use uuid::Uuid;

use crate::{
    query::{read_all_lines, tail_lines},
    rotation::prune_archive,
};

pub struct FlightRecorder {
    tmux: Arc<TmuxClient>,
    storage: Arc<dyn StorageBackend>,
    config: RecorderConfig,
    active_panes: Arc<RwLock<HashMap<Uuid, TmuxTarget>>>,
}

impl FlightRecorder {
    pub fn new(
        tmux: Arc<TmuxClient>,
        storage: Arc<dyn StorageBackend>,
        config: RecorderConfig,
    ) -> Self {
        Self {
            tmux,
            storage,
            config,
            active_panes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn active_target(&self, agent_id: Uuid) -> Result<Option<TmuxTarget>> {
        let active_panes = self.active_panes.read().map_err(lock_error)?;
        Ok(active_panes.get(&agent_id).cloned())
    }
}

impl Recorder for FlightRecorder {
    fn attach(&self, agent: &Agent) -> Result<()> {
        let log_path = self.storage.agent_log_path(agent.agent_id);
        let parent = log_path.parent().ok_or_else(|| AegisError::StorageIo {
            path: log_path.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log path has no parent directory",
            ),
        })?;
        std::fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
            path: parent.to_owned(),
            source,
        })?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|source| AegisError::StorageIo {
                path: log_path.clone(),
                source,
            })?;

        let target = TmuxTarget::parse(&agent.tmux_target())?;
        let tmux = self.tmux.clone();
        let pipe_target = target.clone();
        let pipe_log_path = log_path.clone();
        block_on_tmux(async move { tmux.pipe_attach(&pipe_target, &pipe_log_path).await })?;

        let mut active_panes = self.active_panes.write().map_err(lock_error)?;
        active_panes.insert(agent.agent_id, target);
        info!(agent_id = %agent.agent_id, log = %log_path.display(), "flight recorder attached");
        Ok(())
    }

    fn detach(&self, agent_id: Uuid) -> Result<()> {
        let target = {
            let active_panes = self.active_panes.read().map_err(lock_error)?;
            active_panes.get(&agent_id).cloned()
        };

        let Some(target) = target else {
            return Ok(());
        };

        let tmux = self.tmux.clone();
        let detach_target = target.clone();
        block_on_tmux(async move { tmux.pipe_detach(&detach_target).await })?;

        let mut active_panes = self.active_panes.write().map_err(lock_error)?;
        active_panes.remove(&agent_id);
        info!(agent_id = %agent_id, target = %target, "flight recorder detached");
        Ok(())
    }

    fn archive(&self, agent_id: Uuid) -> Result<PathBuf> {
        let src = self.storage.agent_log_path(agent_id);
        if !src.exists() {
            return Err(AegisError::LogFileNotFound {
                agent_id,
                path: src,
            });
        }

        let archive_dir = self.storage.archive_dir();
        std::fs::create_dir_all(&archive_dir).map_err(|source| AegisError::StorageIo {
            path: archive_dir.clone(),
            source,
        })?;

        let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
        let mut dest = archive_dir.join(format!("{agent_id}_{ts}.log"));
        if dest.exists() {
            dest = unique_archive_path(&archive_dir, agent_id, &ts.to_string());
        }

        std::fs::rename(&src, &dest).map_err(|source| AegisError::StorageIo {
            path: src.clone(),
            source,
        })?;
        info!(agent_id = %agent_id, archive = %dest.display(), "log archived");
        prune_archive(self.storage.as_ref(), &self.config)?;
        Ok(dest)
    }

    fn query(&self, query: &LogQuery) -> Result<Vec<String>> {
        let log_path = self.storage.agent_log_path(query.agent_id);
        if !log_path.exists() {
            return Err(AegisError::LogFileNotFound {
                agent_id: query.agent_id,
                path: log_path,
            });
        }

        match query.last_n_lines {
            Some(n) => tail_lines(&log_path, n),
            None => read_all_lines(&log_path),
        }
    }

    fn log_path(&self, agent_id: Uuid) -> PathBuf {
        self.storage.agent_log_path(agent_id)
    }
}

fn block_on_tmux<F, T>(future: F) -> Result<T>
where
    F: Future<Output = std::result::Result<T, aegis_tmux::TmuxError>> + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(AegisError::Io)?
            .block_on(future)
            .map_err(AegisError::from)
    })
    .join()
    .map_err(|_| AegisError::Io(std::io::Error::other("tmux runtime thread panicked")))?
}

fn unique_archive_path(archive_dir: &std::path::Path, agent_id: Uuid, ts: &str) -> PathBuf {
    for suffix in 1.. {
        let path = archive_dir.join(format!("{agent_id}_{ts}_{suffix}.log"));
        if !path.exists() {
            return path;
        }
    }
    unreachable!("unbounded suffix search should find an archive path")
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> AegisError {
    AegisError::Io(std::io::Error::other(
        "flight recorder active pane lock poisoned",
    ))
}
