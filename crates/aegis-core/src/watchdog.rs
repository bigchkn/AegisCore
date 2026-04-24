use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum DetectedEvent {
    RateLimit {
        agent_id: Uuid,
        matched_pattern: String,
    },
    AuthFailure {
        agent_id: Uuid,
        matched_pattern: String,
    },
    CliCrash {
        agent_id: Uuid,
        exit_code: Option<i32>,
    },
    SandboxViolation {
        agent_id: Uuid,
        matched_pattern: String,
    },
    TaskComplete {
        agent_id: Uuid,
        matched_pattern: String,
    },
}

impl DetectedEvent {
    pub fn agent_id(&self) -> Uuid {
        match self {
            Self::RateLimit { agent_id, .. }
            | Self::AuthFailure { agent_id, .. }
            | Self::CliCrash { agent_id, .. }
            | Self::SandboxViolation { agent_id, .. }
            | Self::TaskComplete { agent_id, .. } => *agent_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchdogAction {
    InitiateFailover,
    PauseAndNotify,
    CaptureAndMarkFailed,
    LogAndContinue,
    TriggerReceiptProcessing,
}

/// Receives detected events from the Watchdog monitor.
/// Implemented by the Controller, which decides and executes the action.
pub trait WatchdogSink: Send + Sync {
    fn on_event(&self, event: DetectedEvent) -> WatchdogAction;
}
