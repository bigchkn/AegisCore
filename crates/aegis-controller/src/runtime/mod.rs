use std::{path::PathBuf, sync::Arc};

use aegis_core::{AgentRegistry, EffectiveConfig, Recorder, Result, SandboxProfile, StorageBackend};
use aegis_providers::ProviderRegistry;
use aegis_recorder::FlightRecorder;
use aegis_sandbox::SeatbeltSandbox;
use aegis_taskflow::TaskflowEngine;
use aegis_tmux::TmuxClient;
use aegis_watchdog::{FailoverExecutor, Watchdog};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    clarification::ClarificationService,
    commands::ControllerCommands,
    daemon::logs::{LogTailer, PaneRelay},
    daemon::projects::ProjectRegistry,
    dispatcher::Dispatcher,
    events::EventBus,
    messaging::MessageRouter,
    prompts::PromptManager,
    registry::FileRegistry,
    scheduler::Scheduler,
    state::StateManager,
    storage::ProjectStorage,
    watchdog::ControllerWatchdogSink,
};

#[derive(Clone)]
pub struct AegisRuntime {
    pub project_id: Uuid,
    pub root_path: PathBuf,
    pub config: EffectiveConfig,
    pub storage: Arc<ProjectStorage>,
    pub registry: Arc<FileRegistry>,
    pub tmux: Arc<TmuxClient>,
    pub sandbox: Arc<dyn SandboxProfile>,
    pub recorder: Arc<dyn Recorder>,
    pub providers: Arc<ProviderRegistry>,
    pub prompts: Arc<PromptManager>,
    pub message_router: Arc<MessageRouter>,
    pub clarifications: Arc<ClarificationService>,
    pub dispatcher: Arc<Dispatcher>,
    pub scheduler: Arc<Scheduler>,
    pub state: Arc<StateManager>,
    pub watchdog_sink: Arc<ControllerWatchdogSink>,
    pub watchdog_shutdown: watch::Sender<bool>,
    pub taskflow: Option<Arc<TaskflowEngine>>,
    pub log_tailer: Arc<LogTailer>,
    pub pane_relay: Arc<PaneRelay>,
    pub events: EventBus,
}

impl AegisRuntime {
    pub async fn build(
        root_path: PathBuf,
        project_registry: Option<Arc<ProjectRegistry>>,
        project_id: Option<Uuid>,
    ) -> Result<Self> {
        let global = EffectiveConfig::load_global()?;
        let project = EffectiveConfig::load_project(&root_path)?;
        let config = EffectiveConfig::resolve(&global, &project)?;
        let validation_errors = config.validate();
        if let Some(error) = validation_errors.first() {
            return Err(aegis_core::AegisError::ConfigValidation {
                field: error.field.clone(),
                reason: error.reason.clone(),
            });
        }

        Self::from_config(root_path, config, project_registry, project_id).await
    }

    pub async fn load(
        root_path: PathBuf,
        project_registry: Option<Arc<ProjectRegistry>>,
        project_id: Option<Uuid>,
    ) -> Result<Self> {
        Self::build(root_path, project_registry, project_id).await
    }

    pub async fn from_config(
        root_path: PathBuf,
        config: EffectiveConfig,
        project_registry: Option<Arc<ProjectRegistry>>,
        project_id: Option<Uuid>,
    ) -> Result<Self> {
        let storage = Arc::new(ProjectStorage::new(root_path.clone()));
        storage.ensure_layout()?;
        FileRegistry::init(storage.as_ref())?;

        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let tmux = Arc::new(TmuxClient::new());
        let sandbox: Arc<dyn SandboxProfile> =
            Arc::new(SeatbeltSandbox::with_logs_dir(storage.logs_dir()));
        let recorder: Arc<dyn Recorder> = Arc::new(FlightRecorder::new(
            tmux.clone(),
            storage.clone(),
            config.recorder.clone(),
        ));
        let providers = Arc::new(ProviderRegistry::from_config(&config)?);
        let prompts = Arc::new(PromptManager::new(root_path.clone()));
        let state = Arc::new(StateManager::new(storage.clone()));
        let taskflow = Arc::new(TaskflowEngine::new(storage.clone(), registry.clone()));
        let message_router = Arc::new(MessageRouter::new(
            registry.clone(),
            storage.clone(),
            Some(tmux.clone()),
        ));
        let clarifications = Arc::new(ClarificationService::new(
            registry.clone(),
            storage.clone(),
            message_router.clone(),
        ));
        let log_tailer = Arc::new(LogTailer::new(storage.clone()));
        let pane_relay = Arc::new(PaneRelay::new(
            storage.clone(),
            registry.clone(),
            tmux.clone(),
        ));
        let events = EventBus::default();
        let watchdog_sink = Arc::new(ControllerWatchdogSink::new(
            registry.clone(),
            events.clone(),
            config.watchdog.failover_enabled,
        ));
        let (watchdog_shutdown, _) = watch::channel(false);

        let dispatcher = Arc::new(Dispatcher::new(
            registry.clone(),
            project_registry,
            project_id,
            Some(tmux.clone()),
            Some(sandbox.clone()),
            Some(recorder.clone()),
            providers.clone(),
            prompts.clone(),
            storage.clone(),
            events.clone(),
            config.clone(),
        ));
        let scheduler = Arc::new(Scheduler::new(
            registry.clone(),
            dispatcher.clone(),
            config.global.max_splinters as usize,
        ));

        Ok(Self {
            project_id: project_id.unwrap_or_else(Uuid::new_v4),
            root_path,
            config,
            storage,
            registry,
            tmux,
            sandbox,
            recorder,
            providers,
            prompts,
            message_router,
            clarifications,
            dispatcher,
            scheduler,
            state,
            watchdog_sink,
            watchdog_shutdown,
            taskflow: Some(taskflow),
            log_tailer,
            pane_relay,
            events,
        })
    }

    pub async fn recover(&self) -> Result<()> {
        self.state.recover()?;
        let _ = self.clarifications.recover_pending_deliveries().await?;
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        for (name, entry) in &self.config.agents {
            if entry.kind == aegis_core::AgentKind::Bastion {
                self.dispatcher.spawn_bastion(name).await?;
            }
        }
        self.start_background_tasks();
        Ok(())
    }

    /// Start background tasks (scheduler drain loop) without spawning bastions.
    /// Safe to call on a freshly loaded runtime that was not started via `session.start`.
    pub fn start_background_tasks(&self) {
        let scheduler = Arc::clone(&self.scheduler);
        tokio::spawn(async move { scheduler.run_drain_loop("splinter").await });

        let executor: Arc<dyn FailoverExecutor> = self.dispatcher.clone();
        match Watchdog::new(
            self.tmux.clone(),
            self.registry.clone(),
            self.registry.clone(),
            self.recorder.clone(),
            self.providers.clone(),
            self.watchdog_sink.clone(),
            self.config.watchdog.clone(),
            self.config.recorder.clone(),
            executor,
        ) {
            Ok(watchdog) => {
                let shutdown = self.watchdog_shutdown.subscribe();
                tokio::spawn(async move {
                    if let Err(error) = watchdog.run(shutdown).await {
                        tracing::error!(%error, "watchdog task stopped");
                    }
                });
            }
            Err(error) => {
                tracing::error!(%error, "failed to start watchdog");
            }
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.watchdog_shutdown.send(true);
        self.state.snapshot_now()?;
        self.kill_all_agent_sessions().await;
        Ok(())
    }

    async fn kill_all_agent_sessions(&self) {
        let agents = match self.registry.list_active() {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(%e, "could not list agents during shutdown");
                return;
            }
        };
        for agent in agents {
            if let Err(e) = self.tmux.kill_session(&agent.tmux_session).await {
                tracing::debug!(session = %agent.tmux_session, %e, "tmux kill_session during shutdown");
            }
        }
    }

    pub fn commands(&self) -> ControllerCommands {
        ControllerCommands::new(
            self.registry.clone(),
            self.dispatcher.clone(),
            self.message_router.clone(),
            self.clarifications.clone(),
            self.scheduler.clone(),
            Some(self.recorder.clone()),
            self.taskflow.clone(),
        )
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<aegis_core::AegisEvent> {
        self.events.subscribe()
    }
}
