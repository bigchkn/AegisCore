pub mod commands;
pub mod daemon;
pub mod dispatcher;
pub mod events;
pub mod git;
pub mod lifecycle;
pub mod prompts;
pub mod registry;
pub mod runtime;
pub mod scheduler;
pub mod state;
pub mod storage;
pub mod watchdog;

pub use commands::{ControllerCommands, ProjectStatus};
pub use dispatcher::Dispatcher;
pub use events::EventBus;
pub use git::GitWorktree;
pub use lifecycle::{AgentSpec, RunningAgent, SpawnPlan};
pub use prompts::{PromptContext, PromptManager, PromptType};
pub use registry::FileRegistry;
pub use runtime::AegisRuntime;
pub use scheduler::Scheduler;
pub use state::{RecoveryResult, StateManager};
pub use storage::ProjectStorage;
pub use watchdog::ControllerWatchdogSink;

#[cfg(all(test, feature = "ts-export"))]
mod ts_export {
    use crate::commands::ProjectStatus;
    use crate::daemon::projects::ProjectRecord;
    use ts_rs::TS;

    #[test]
    fn export_ts_bindings() {
        let out = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/aegis-web/frontend/src/types"
        );
        std::fs::create_dir_all(out).unwrap();
        ProjectRecord::export_all_to(out).unwrap();
        ProjectStatus::export_all_to(out).unwrap();
    }
}
