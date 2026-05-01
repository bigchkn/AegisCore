use aegis_core::{Agent, Result};
use aegis_tmux::{TmuxClient, TmuxTarget};
use std::sync::Arc;

pub struct ObservationService {
    tmux: Arc<TmuxClient>,
}

impl ObservationService {
    pub fn new(tmux: Arc<TmuxClient>) -> Self {
        Self { tmux }
    }

    /// Scrape the last N lines from an agent's pane.
    pub async fn scrape(&self, agent: &Agent, lines: usize) -> Result<String> {
        let target = TmuxTarget::new(&agent.tmux_session, agent.tmux_window, &agent.tmux_pane);
        self.tmux
            .capture_pane_plain(&target, lines)
            .await
            .map_err(aegis_core::error::AegisError::from)
    }
}
