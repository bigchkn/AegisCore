use std::{
    io::{ErrorKind, Write},
    path::Path,
};

use tracing::debug;
use tokio::time::{sleep, Duration};

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

    /// Enable tmux options that reduce TUI interference for interactive panes.
    pub async fn harden_pane(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.run_tmux(&["set-option", "-pt", target.as_str(), "allow-passthrough", "on"])
            .await?;
        self.run_tmux(&["set-option", "-pt", target.as_str(), "extended-keys", "on"])
            .await?;
        Ok(())
    }

    /// Wait until the pane content and cursor stop changing for a sustained period.
    pub async fn wait_for_stability(
        &self,
        target: &TmuxTarget,
        stable_duration_ms: u64,
        polling_interval_ms: u64,
        timeout_ms: u64,
    ) -> Result<bool, TmuxError> {
        let required_checks = std::cmp::max(
            1,
            ((stable_duration_ms + polling_interval_ms - 1) / polling_interval_ms) as usize,
        );
        let mut last_signature = String::new();
        let mut stable_checks = 0usize;
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);

        while std::time::Instant::now() < deadline {
            sleep(Duration::from_millis(polling_interval_ms)).await;

            let content = match self.capture_pane_plain(target, 200).await {
                Ok(content) => content,
                Err(_) => continue,
            };
            let cursor = match self
                .run_tmux(&[
                    "display-message",
                    "-t",
                    target.as_str(),
                    "-p",
                    "#{cursor_x},#{cursor_y}",
                ])
                .await
            {
                Ok(cursor) => cursor.trim().to_string(),
                Err(_) => continue,
            };

            let signature = format!("{content}\n__CURSOR__:{cursor}");
            if signature == last_signature {
                stable_checks += 1;
            } else {
                stable_checks = 0;
                last_signature = signature;
            }

            if stable_checks >= required_checks {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Clear the current line and type a prompt the way a human would.
    pub async fn send_interactive_text(
        &self,
        target: &TmuxTarget,
        text: &str,
    ) -> Result<(), TmuxError> {
        self.send_key(target, "Escape").await?;
        sleep(Duration::from_millis(100)).await;
        self.send_key(target, "C-u").await?;
        sleep(Duration::from_millis(200)).await;

        for ch in text.chars() {
            let literal = ch.to_string();
            self.run_tmux(&["send-keys", "-t", target.as_str(), "-l", &literal])
                .await?;
            sleep(Duration::from_millis(20)).await;
        }

        sleep(Duration::from_millis(500)).await;
        self.send_key(target, "Enter").await?;
        Ok(())
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

    /// Resize a specific pane.
    pub async fn resize_pane(
        &self,
        target: &TmuxTarget,
        cols: u32,
        rows: u32,
    ) -> Result<(), TmuxError> {
        self.run_tmux(&[
            "resize-pane",
            "-t",
            target.as_str(),
            "-x",
            &cols.to_string(),
            "-y",
            &rows.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Inject arbitrary raw bytes (e.g. control sequences, paste data) into the pane.
    /// Uses tmux load-buffer + paste-buffer for reliable injection without shell escaping.
    pub async fn send_raw_input(&self, target: &TmuxTarget, data: &[u8]) -> Result<(), TmuxError> {
        let mut tmp = tempfile::NamedTempFile::new().map_err(|e| TmuxError::Io { source: e })?;
        tmp.write_all(data)
            .map_err(|e| TmuxError::Io { source: e })?;
        tmp.flush().map_err(|e| TmuxError::Io { source: e })?;

        let buffer_name = format!("aegis_paste_{}", uuid::Uuid::new_v4());
        let path_str = tmp.path().to_string_lossy();

        // 1. Load temp file into tmux buffer
        self.run_tmux(&["load-buffer", "-b", &buffer_name, &path_str])
            .await?;

        // 2. Paste buffer into target pane
        // -r: do not replace LF with CR (raw)
        self.run_tmux(&[
            "paste-buffer",
            "-b",
            &buffer_name,
            "-r",
            "-t",
            target.as_str(),
        ])
        .await?;

        // 3. Clean up buffer
        let _ = self.run_tmux(&["delete-buffer", "-b", &buffer_name]).await;

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

    /// Returns the name of the current foreground command in the pane
    /// (e.g. `"claude"`, `"gemini"`, `"zsh"`).
    pub async fn pane_current_command(
        &self,
        target: &TmuxTarget,
    ) -> Result<String, TmuxError> {
        let out = self
            .run_tmux(&[
                "display-message",
                "-t",
                target.as_str(),
                "-p",
                "#{pane_current_command}",
            ])
            .await?;
        Ok(out.trim().to_string())
    }

    /// Returns true if the pane exists and the foreground command is NOT a login shell.
    ///
    /// This is used to distinguish a pane that still has an agent CLI running from one
    /// where the CLI has exited and dropped back to the shell prompt.
    pub async fn pane_has_agent(&self, target: &TmuxTarget) -> Result<bool, TmuxError> {
        if !self.pane_is_alive(target).await? {
            return Ok(false);
        }
        let cmd = self.pane_current_command(target).await?;
        let is_shell = matches!(
            cmd.as_str(),
            "bash" | "zsh" | "sh" | "fish" | "dash" | "csh" | "tcsh"
        );
        Ok(!is_shell)
    }
}

impl Default for TmuxClient {
    fn default() -> Self {
        Self::new()
    }
}
