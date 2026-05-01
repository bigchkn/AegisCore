use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use aegis_core::{
    provider::{NudgeAction, NudgeDefinition, NudgeTrigger},
    Agent, Result,
};
use aegis_tmux::{TmuxClientInterface, TmuxTarget};
use tracing::{debug, info};
use uuid::Uuid;

use crate::FailoverExecutor;

pub struct NudgeManager {
    tmux: Arc<dyn TmuxClientInterface>,
    executor: Arc<dyn FailoverExecutor>,
    /// Last output activity per agent
    last_activity: Mutex<HashMap<Uuid, Instant>>,
    /// Last nudge execution per agent and nudge index
    last_nudge: Mutex<HashMap<(Uuid, usize), Instant>>,
}

impl NudgeManager {
    pub fn new(tmux: Arc<dyn TmuxClientInterface>, executor: Arc<dyn FailoverExecutor>) -> Self {
        Self {
            tmux,
            executor,
            last_activity: Mutex::new(HashMap::new()),
            last_nudge: Mutex::new(HashMap::new()),
        }
    }

    pub fn record_activity(&self, agent_id: Uuid) {
        let mut activity = self.last_activity.lock().unwrap();
        activity.insert(agent_id, Instant::now());
    }

    pub async fn check_and_apply(
        &self,
        agent: &Agent,
        nudges: &[NudgeDefinition],
        current_screen: &str,
    ) -> Result<()> {
        let now = Instant::now();
        let target = TmuxTarget::parse(&agent.tmux_target())?;

        for (idx, nudge) in nudges.iter().enumerate() {
            let key = (agent.agent_id, idx);

            // If not repeatable, check if already nudged
            if !nudge.repeat {
                let last = self.last_nudge.lock().unwrap();
                if last.contains_key(&key) {
                    continue;
                }
            }

            let should_trigger = match &nudge.trigger {
                NudgeTrigger::Stalled { timeout_ms } => {
                    let last = {
                        let activity = self.last_activity.lock().unwrap();
                        // Fallback to now() if we haven't seen any activity yet,
                        // so we don't immediately nudge on spawn.
                        *activity.get(&agent.agent_id).unwrap_or(&now)
                    };
                    now.duration_since(last) >= Duration::from_millis(*timeout_ms)
                }
                NudgeTrigger::Pattern(pattern) => current_screen.contains(pattern),
                NudgeTrigger::ScreenScrape {
                    pattern, region, ..
                } => {
                    // For now, ScreenScrape is similar to Pattern but could be refined with region slicing
                    if let Some(_rect) = region {
                        // TODO: Implement region-based slicing if TmuxClient supports raw grid capture
                        // For now fallback to simple pattern match
                        current_screen.contains(pattern)
                    } else {
                        current_screen.contains(pattern)
                    }
                }
                NudgeTrigger::Stability {
                    stable_ms,
                    timeout_ms,
                } => self
                    .tmux
                    .wait_for_stability(&target, *stable_ms, 250, *timeout_ms)
                    .await
                    .unwrap_or(false),
                NudgeTrigger::TaskComplete => {
                    // TaskComplete nudges are handled explicitly via handle_event,
                    // not in the periodic sweep.
                    false
                }
            };

            if should_trigger {
                // Throttle nudges to avoid spamming (e.g. once every 5 seconds)
                {
                    let last = self.last_nudge.lock().unwrap();
                    if let Some(prev) = last.get(&key) {
                        if now.duration_since(*prev) < Duration::from_secs(5) {
                            continue;
                        }
                    }
                }

                info!(agent_id = %agent.agent_id, nudge_idx = idx, "triggering nudge");
                self.apply_actions(agent, &target, &nudge.actions).await?;

                let mut last = self.last_nudge.lock().unwrap();
                last.insert(key, now);
            }
        }

        Ok(())
    }

    pub async fn apply_actions(
        &self,
        agent: &Agent,
        target: &TmuxTarget,
        actions: &[NudgeAction],
    ) -> Result<()> {
        for action in actions {
            match action {
                NudgeAction::SendText { text } => {
                    debug!(agent_id = %agent.agent_id, text = %text, "nudge: sending text");
                    self.tmux.send_interactive_text(target, text).await?;
                }
                NudgeAction::Wait { duration_ms } => {
                    debug!(agent_id = %agent.agent_id, ms = %duration_ms, "nudge: waiting");
                    tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                }
                NudgeAction::SendInitialPrompt => {
                    debug!(agent_id = %agent.agent_id, "nudge: sending initial prompt");
                    self.executor.send_initial_prompt(agent).await?;
                }
            }
        }
        Ok(())
    }
}
