use crate::daemon::http::{HttpServer, HttpState};
use crate::daemon::projects::ProjectRegistry;
use crate::daemon::uds::UdsServer;
use crate::events::EventBus;
use crate::runtime::AegisRuntime;
use aegis_core::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

pub struct DaemonSupervisor {
    event_bus: Arc<EventBus>,
    project_registry: Arc<ProjectRegistry>,
    active_runtimes: Arc<Mutex<HashMap<Uuid, AegisRuntime>>>,
    uds_path: PathBuf,
    http_port: u16,
}

impl DaemonSupervisor {
    pub fn new(uds_path: PathBuf, http_port: u16) -> Self {
        Self {
            event_bus: Arc::new(EventBus::default()),
            project_registry: Arc::new(ProjectRegistry::new()),
            active_runtimes: Arc::new(Mutex::new(HashMap::new())),
            uds_path,
            http_port,
        }
    }

    pub async fn run(self) -> Result<()> {
        info!("Starting AegisCore Daemon Supervisor");

        // 1. Load auto-start projects
        let projects = self.project_registry.load()?;
        for project in projects {
            if project.auto_start {
                let runtime = AegisRuntime::load(project.root_path.clone()).await?;
                runtime.recover().await?;
                runtime.start().await?;
                self.active_runtimes
                    .lock()
                    .await
                    .insert(project.id, runtime);
                info!(
                    "Auto-started project: {} ({})",
                    project.id,
                    project.root_path.display()
                );
            }
        }

        // 2. Start UDS Server
        let uds_server = UdsServer::bind(
            self.uds_path.clone(),
            Arc::clone(&self.event_bus),
            Arc::clone(&self.project_registry),
            Arc::clone(&self.active_runtimes),
        )
        .await?;
        let uds_handle = tokio::spawn(uds_server.run());

        // 3. Start HTTP Server (optional — failure does not bring down the daemon)
        let http_state = HttpState {
            event_bus: Arc::clone(&self.event_bus),
            projects: Arc::clone(&self.project_registry),
            active_runtimes: Arc::clone(&self.active_runtimes),
        };
        let http_server = HttpServer::new(http_state);
        let port = self.http_port;
        tokio::spawn(async move {
            if let Err(e) = http_server.run(port).await {
                error!("HTTP server error (non-fatal): {}", e);
            }
        });

        info!("Daemon servers initialized");

        // 4. Wait for termination signal
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received (Ctrl+C)");
            }
            _ = uds_handle => {
                error!("UDS server exited unexpectedly");
            }
        }

        self.shutdown().await
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Graceful shutdown initiated");

        // Clean up UDS socket
        if self.uds_path.exists() {
            let _ = std::fs::remove_file(&self.uds_path);
        }

        // Shutdown active runtimes
        let mut runtimes = self.active_runtimes.lock().await;
        for (id, runtime) in runtimes.drain() {
            info!("Shutting down project: {}", id);
            if let Err(e) = runtime.shutdown().await {
                error!("Error shutting down project {}: {}", id, e);
            }
        }

        info!("Daemon shutdown complete");
        Ok(())
    }
}
