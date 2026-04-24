use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aegis_tmux::{TmuxClient, TmuxError, TmuxTarget};
use tokio::time::sleep;

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
        let name = unique_name("aegis-test");
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

fn temp_log_path() -> PathBuf {
    std::env::temp_dir().join(format!("{}.log", unique_name("aegis-tmux-pipe")))
}

#[tokio::test]
async fn send_text_and_capture_plain_round_trip() -> Result<(), TmuxError> {
    if !tmux_available() {
        eprintln!("skipping tmux integration test: tmux not installed");
        return Ok(());
    }

    let client = TmuxClient::new();
    let session = TmuxSession::new(&client).await?;
    let target = session.target();

    client
        .send_text(&target, "printf 'aegis-roundtrip\\n'")
        .await?;
    sleep(Duration::from_millis(200)).await;

    let captured = client.capture_pane_plain(&target, 20).await?;
    assert!(captured.contains("aegis-roundtrip"), "{captured}");

    Ok(())
}

#[tokio::test]
async fn pipe_attach_writes_log_and_detach_stops() -> Result<(), TmuxError> {
    if !tmux_available() {
        eprintln!("skipping tmux integration test: tmux not installed");
        return Ok(());
    }

    let client = TmuxClient::new();
    let session = TmuxSession::new(&client).await?;
    let target = session.target();
    let log_path = temp_log_path();

    client.pipe_attach(&target, &log_path).await?;
    client
        .send_text(&target, "printf 'pipe-before-detach\\n'")
        .await?;
    sleep(Duration::from_millis(200)).await;

    let before = std::fs::read_to_string(&log_path).map_err(|source| TmuxError::Io { source })?;
    assert!(before.contains("pipe-before-detach"), "{before}");

    client.pipe_detach(&target).await?;
    client
        .send_text(&target, "printf 'pipe-after-detach\\n'")
        .await?;
    sleep(Duration::from_millis(200)).await;

    let after = std::fs::read_to_string(&log_path).map_err(|source| TmuxError::Io { source })?;
    assert!(!after.contains("pipe-after-detach"), "{after}");

    let _ = std::fs::remove_file(log_path);
    Ok(())
}

#[tokio::test]
async fn pane_liveness_running() -> Result<(), TmuxError> {
    if !tmux_available() {
        eprintln!("skipping tmux integration test: tmux not installed");
        return Ok(());
    }

    let client = TmuxClient::new();
    let session = TmuxSession::new(&client).await?;
    let target = session.target();

    assert!(client.pane_is_alive(&target).await?);
    assert_eq!(client.pane_exit_status(&target).await?, None);

    Ok(())
}

#[tokio::test]
async fn missing_binary_maps_to_binary_not_found() {
    let client = TmuxClient::with_binary("__aegis_missing_tmux_binary__");
    let result = client.session_exists("anything").await;
    assert!(
        matches!(result, Err(TmuxError::BinaryNotFound { .. })),
        "{result:?}"
    );
}
