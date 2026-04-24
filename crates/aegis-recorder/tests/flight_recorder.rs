use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aegis_core::{
    config::RecorderConfig, AegisError, Agent, AgentKind, AgentStatus, LogQuery, Recorder,
    StorageBackend,
};
use aegis_recorder::FlightRecorder;
use aegis_tmux::{TmuxClient, TmuxError, TmuxTarget};
use chrono::Utc;
use tempfile::tempdir;
use tokio::time::sleep;
use uuid::Uuid;

struct TestStorage {
    root: PathBuf,
}

impl StorageBackend for TestStorage {
    fn project_root(&self) -> &Path {
        &self.root
    }
}

fn config(retention: usize) -> RecorderConfig {
    RecorderConfig {
        failover_context_lines: 100,
        log_rotation_max_mb: 50,
        log_retention_count: retention,
    }
}

fn recorder(root: PathBuf, config: RecorderConfig) -> FlightRecorder {
    FlightRecorder::new(
        Arc::new(TmuxClient::new()),
        Arc::new(TestStorage { root }),
        config,
    )
}

fn agent(agent_id: Uuid, session: &str) -> Agent {
    Agent {
        agent_id,
        name: "test-agent".to_owned(),
        kind: AgentKind::Bastion,
        status: AgentStatus::Active,
        role: "tester".to_owned(),
        parent_id: None,
        task_id: None,
        tmux_session: session.to_owned(),
        tmux_window: 0,
        tmux_pane: "0".to_owned(),
        worktree_path: PathBuf::from("/tmp"),
        cli_provider: "codex".to_owned(),
        fallback_cascade: Vec::new(),
        sandbox_profile: PathBuf::from("/tmp/test.sb"),
        log_path: PathBuf::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        terminated_at: None,
    }
}

#[test]
fn query_entire_log() {
    let dir = tempdir().unwrap();
    let recorder = recorder(dir.path().to_owned(), config(20));
    let agent_id = Uuid::new_v4();
    let path = recorder.log_path(agent_id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();

    let lines = recorder
        .query(&LogQuery {
            agent_id,
            last_n_lines: None,
            since: None,
            follow: false,
        })
        .unwrap();

    assert_eq!(lines, vec!["one", "two", "three"]);
}

#[test]
fn query_last_n_lines() {
    let dir = tempdir().unwrap();
    let recorder = recorder(dir.path().to_owned(), config(20));
    let agent_id = Uuid::new_v4();
    let path = recorder.log_path(agent_id);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = (0..25)
        .map(|line| format!("line-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, content).unwrap();

    let lines = recorder
        .query(&LogQuery {
            agent_id,
            last_n_lines: Some(5),
            since: None,
            follow: false,
        })
        .unwrap();

    assert_eq!(
        lines,
        vec!["line-20", "line-21", "line-22", "line-23", "line-24"]
    );
}

#[test]
fn query_missing_log() {
    let dir = tempdir().unwrap();
    let recorder = recorder(dir.path().to_owned(), config(20));
    let agent_id = Uuid::new_v4();

    let err = recorder
        .query(&LogQuery {
            agent_id,
            last_n_lines: Some(10),
            since: None,
            follow: false,
        })
        .unwrap_err();

    assert!(matches!(err, AegisError::LogFileNotFound { .. }));
}

#[test]
fn archive_moves_log_and_prunes() {
    let dir = tempdir().unwrap();
    let recorder = recorder(dir.path().to_owned(), config(1));
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();

    for (agent_id, content) in [(first_id, "first"), (second_id, "second")] {
        let path = recorder.log_path(agent_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        let archived = recorder.archive(agent_id).unwrap();
        assert!(archived.exists());
        assert!(!path.exists());
        std::thread::sleep(Duration::from_millis(2));
    }

    let archive_dir = dir.path().join(".aegis").join("logs").join("archive");
    let remaining = std::fs::read_dir(&archive_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].starts_with(&second_id.to_string()));
}

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

struct TmuxSession {
    name: String,
}

impl TmuxSession {
    async fn new(client: &TmuxClient) -> Result<Self, TmuxError> {
        let name = unique_name("aegis-recorder-test");
        client.new_session(&name).await?;
        Ok(Self { name })
    }

    fn target(&self) -> TmuxTarget {
        TmuxTarget::new(&self.name, 0, "0")
    }
}

impl Drop for TmuxSession {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn attach_captures_output_and_detach_stops() -> Result<(), Box<dyn std::error::Error>> {
    if !tmux_available() {
        eprintln!("skipping tmux integration test: tmux not installed");
        return Ok(());
    }

    let dir = tempdir().unwrap();
    let storage = Arc::new(TestStorage {
        root: dir.path().to_owned(),
    });
    let tmux = Arc::new(TmuxClient::new());
    let recorder = FlightRecorder::new(tmux.clone(), storage, config(20));
    let session = TmuxSession::new(&tmux).await?;
    let agent_id = Uuid::new_v4();
    let agent = agent(agent_id, &session.name);

    recorder.attach(&agent)?;
    let log_path = recorder.log_path(agent_id);
    assert!(log_path.exists());
    assert_eq!(
        recorder
            .active_target(agent_id)?
            .map(|target| target.to_string()),
        Some(session.target().to_string())
    );

    tmux.send_text(&session.target(), "printf 'recorder-before-detach\\n'")
        .await?;
    sleep(Duration::from_millis(250)).await;
    let before = std::fs::read_to_string(&log_path)?;
    assert!(before.contains("recorder-before-detach"), "{before}");

    recorder.detach(agent_id)?;
    assert!(recorder.active_target(agent_id)?.is_none());

    tmux.send_text(&session.target(), "printf 'recorder-after-detach\\n'")
        .await?;
    sleep(Duration::from_millis(250)).await;
    let after = std::fs::read_to_string(&log_path)?;
    assert!(!after.contains("recorder-after-detach"), "{after}");

    Ok(())
}
