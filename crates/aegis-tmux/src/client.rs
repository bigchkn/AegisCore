use std::{io::ErrorKind, path::Path};

use tracing::debug;

use crate::{escape::escape_for_send_keys, TmuxError, TmuxTarget};

pub struct TmuxClient {
    tmux_bin: String,
}

impl TmuxClient {
    pub fn new() -> Self {
        Self {
            tmux_bin: "tmux".to_owned(),
        }
    }

    pub fn with_binary(bin: &str) -> Self {
        Self {
            tmux_bin: bin.to_owned(),
        }
    }

    // ── internal ──────────────────────────────────────────────────────────────

    async fn run_tmux(&self, args: &[&str]) -> Result<String, TmuxError> {
        debug!(bin = %self.tmux_bin, ?args, "tmux");
        let output = tokio::process::Command::new(&self.tmux_bin)
            .args(args)
            .output()
            .await
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound | ErrorKind::PermissionDenied => TmuxError::BinaryNotFound {
                    reason: e.to_string(),
                },
                _ => TmuxError::Io { source: e },
            })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(TmuxError::CommandFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    // ── session / window / pane lifecycle ─────────────────────────────────────

    /// Create a new detached tmux session. Returns the session name.
    pub async fn new_session(&self, name: &str) -> Result<String, TmuxError> {
        self.run_tmux(&["new-session", "-d", "-s", name]).await?;
        Ok(name.to_owned())
    }

    /// Create a new window in an existing session. Returns the window index.
    pub async fn new_window(&self, session: &str, name: Option<&str>) -> Result<u32, TmuxError> {
        let target = format!("{session}:");
        let mut args = vec!["new-window", "-t", &target, "-P", "-F", "#{window_index}"];
        if let Some(n) = name {
            args.extend(["-n", n]);
        }
        let out = self.run_tmux(&args).await?;
        out.trim()
            .parse::<u32>()
            .map_err(|_| TmuxError::CommandFailed {
                code: -1,
                stderr: format!("unexpected window_index output: {}", out.trim()),
            })
    }

    /// Split an existing window, creating a new pane. Returns the pane ID (`%N`).
    pub async fn split_window(&self, target: &TmuxTarget) -> Result<String, TmuxError> {
        let out = self
            .run_tmux(&[
                "split-window",
                "-t",
                target.as_str(),
                "-P",
                "-F",
                "#{pane_id}",
            ])
            .await?;
        Ok(out.trim().to_owned())
    }

    /// Kill a specific pane and its process.
    pub async fn kill_pane(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.run_tmux(&["kill-pane", "-t", target.as_str()]).await?;
        Ok(())
    }

    /// Kill an entire window and all its panes.
    pub async fn kill_window(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.run_tmux(&["kill-window", "-t", target.as_str()])
            .await?;
        Ok(())
    }

    /// Kill an entire session.
    pub async fn kill_session(&self, session: &str) -> Result<(), TmuxError> {
        self.run_tmux(&["kill-session", "-t", session]).await?;
        Ok(())
    }

    /// Returns true if the named session exists.
    pub async fn session_exists(&self, session: &str) -> Result<bool, TmuxError> {
        debug!(bin = %self.tmux_bin, session, "tmux has-session");
        let status = tokio::process::Command::new(&self.tmux_bin)
            .args(["has-session", "-t", session])
            .status()
            .await
            .map_err(|e| match e.kind() {
                ErrorKind::NotFound | ErrorKind::PermissionDenied => TmuxError::BinaryNotFound {
                    reason: e.to_string(),
                },
                _ => TmuxError::Io { source: e },
            })?;
        Ok(status.success())
    }

    /// Returns the pane IDs of all panes in a window.
    pub async fn list_panes(&self, target: &TmuxTarget) -> Result<Vec<String>, TmuxError> {
        let out = self
            .run_tmux(&["list-panes", "-t", target.as_str(), "-F", "#{pane_id}"])
            .await?;
        Ok(out
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect())
    }

    // ── send-keys ─────────────────────────────────────────────────────────────

    /// Inject text into the pane as if typed, followed by Enter.
    /// Uses `-l` (literal) flag so tmux key bindings are not triggered.
    pub async fn send_text(&self, target: &TmuxTarget, text: &str) -> Result<(), TmuxError> {
        self.run_tmux(&["send-keys", "-t", target.as_str(), "-l", text])
            .await?;
        self.run_tmux(&["send-keys", "-t", target.as_str(), "Enter"])
            .await?;
        Ok(())
    }

    /// Send a named key event (e.g. `"C-c"`, `"Enter"`, `"Escape"`).
    /// Does not use `-l` — intended for control sequences only.
    pub async fn send_key(&self, target: &TmuxTarget, key: &str) -> Result<(), TmuxError> {
        self.run_tmux(&["send-keys", "-t", target.as_str(), key])
            .await?;
        Ok(())
    }

    /// Send Ctrl-C to interrupt the running process in the pane.
    pub async fn interrupt(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.run_tmux(&["send-keys", "-t", target.as_str(), "C-c"])
            .await?;
        Ok(())
    }

    // ── capture-pane ──────────────────────────────────────────────────────────

    /// Capture the last `lines` lines of terminal output, including ANSI sequences.
    pub async fn capture_pane(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError> {
        let start = format!("-{lines}");
        self.run_tmux(&["capture-pane", "-t", target.as_str(), "-p", "-S", &start])
            .await
    }

    /// Capture the last `lines` lines with ANSI escape codes stripped.
    pub async fn capture_pane_plain(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError> {
        let start = format!("-{lines}");
        self.run_tmux(&[
            "capture-pane",
            "-t",
            target.as_str(),
            "-p",
            "-e",
            "-S",
            &start,
        ])
        .await
    }

    // ── pipe-pane ─────────────────────────────────────────────────────────────

    /// Attach a log stream: all pane output is appended to `log_path`.
    /// Idempotent — calling again on an already-piped pane is a no-op.
    pub async fn pipe_attach(&self, target: &TmuxTarget, log_path: &Path) -> Result<(), TmuxError> {
        let path_escaped = escape_for_send_keys(&log_path.to_string_lossy());
        let shell_cmd = format!("cat >> {path_escaped}");
        self.run_tmux(&["pipe-pane", "-t", target.as_str(), "-o", &shell_cmd])
            .await?;
        Ok(())
    }

    /// Detach the log stream from the pane. Idempotent.
    pub async fn pipe_detach(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.run_tmux(&["pipe-pane", "-t", target.as_str()]).await?;
        Ok(())
    }

    // ── pane liveness ─────────────────────────────────────────────────────────

    /// Returns the exit status of the process in the pane.
    /// `None` = still running. `Some(code)` = exited with that code.
    pub async fn pane_exit_status(&self, target: &TmuxTarget) -> Result<Option<i32>, TmuxError> {
        let out = self
            .run_tmux(&[
                "display-message",
                "-t",
                target.as_str(),
                "-p",
                "#{pane_dead} #{pane_dead_status}",
            ])
            .await?;
        let trimmed = out.trim();
        let mut parts = trimmed.splitn(2, ' ');
        match (parts.next(), parts.next()) {
            (Some("0"), _) => Ok(None),
            (Some("1"), Some(code)) => Ok(Some(code.trim().parse::<i32>().unwrap_or(-1))),
            _ => Ok(None),
        }
    }

    /// Returns true if the pane exists and its process is still running.
    pub async fn pane_is_alive(&self, target: &TmuxTarget) -> Result<bool, TmuxError> {
        Ok(self.pane_exit_status(target).await?.is_none())
    }
}

impl Default for TmuxClient {
    fn default() -> Self {
        Self::new()
    }
}
