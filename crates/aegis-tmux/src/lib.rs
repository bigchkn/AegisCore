mod client;
mod error;
mod escape;
mod target;

pub use client::TmuxClient;
pub use error::TmuxError;
pub use escape::escape_for_send_keys;
pub use target::TmuxTarget;

use async_trait::async_trait;

#[async_trait]
pub trait TmuxClientInterface: Send + Sync {
    async fn send_interactive_text(&self, target: &TmuxTarget, text: &str)
        -> Result<(), TmuxError>;
    async fn capture_pane_plain(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError>;
    async fn pane_exit_status(&self, target: &TmuxTarget) -> Result<Option<i32>, TmuxError>;
    async fn session_exists(&self, session: &str) -> Result<bool, TmuxError>;
    async fn kill_session(&self, session: &str) -> Result<(), TmuxError>;
    async fn new_session(&self, name: &str) -> Result<String, TmuxError>;
    async fn harden_pane(&self, target: &TmuxTarget) -> Result<(), TmuxError>;
    async fn list_panes(&self, target: &TmuxTarget) -> Result<Vec<String>, TmuxError>;
    async fn pane_has_agent(&self, target: &TmuxTarget) -> Result<bool, TmuxError>;
    async fn wait_for_stability(
        &self,
        target: &TmuxTarget,
        stable_duration_ms: u64,
        polling_interval_ms: u64,
        timeout_ms: u64,
    ) -> Result<bool, TmuxError>;
}

#[async_trait]
impl TmuxClientInterface for TmuxClient {
    async fn send_interactive_text(
        &self,
        target: &TmuxTarget,
        text: &str,
    ) -> Result<(), TmuxError> {
        self.send_interactive_text(target, text).await
    }
    async fn capture_pane_plain(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError> {
        self.capture_pane_plain(target, lines).await
    }
    async fn pane_exit_status(&self, target: &TmuxTarget) -> Result<Option<i32>, TmuxError> {
        self.pane_exit_status(target).await
    }
    async fn session_exists(&self, session: &str) -> Result<bool, TmuxError> {
        self.session_exists(session).await
    }
    async fn kill_session(&self, session: &str) -> Result<(), TmuxError> {
        self.kill_session(session).await
    }
    async fn new_session(&self, name: &str) -> Result<String, TmuxError> {
        self.new_session(name).await
    }
    async fn harden_pane(&self, target: &TmuxTarget) -> Result<(), TmuxError> {
        self.harden_pane(target).await
    }
    async fn list_panes(&self, target: &TmuxTarget) -> Result<Vec<String>, TmuxError> {
        self.list_panes(target).await
    }
    async fn pane_has_agent(&self, target: &TmuxTarget) -> Result<bool, TmuxError> {
        self.pane_has_agent(target).await
    }
    async fn wait_for_stability(
        &self,
        target: &TmuxTarget,
        stable_duration_ms: u64,
        polling_interval_ms: u64,
        timeout_ms: u64,
    ) -> Result<bool, TmuxError> {
        self.wait_for_stability(target, stable_duration_ms, polling_interval_ms, timeout_ms)
            .await
    }
}
