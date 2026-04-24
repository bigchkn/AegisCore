use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{AppState, ConnectionStatus};
use crate::client::AegisClient;
use crate::handler::{handle_key_events, AppAction};
use crate::ui;

pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    pub app: AppState,
    client: AegisClient,
}

impl Tui {
    pub fn new(app: AppState, client: AegisClient) -> Result<Self> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal, app, client })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut last_tick = Instant::now();
        let tick_rate = Duration::from_millis(100);

        // Initial connection
        self.app.connection_status = ConnectionStatus::Connecting;
        let mut event_rx = match self.client.subscribe().await {
            Ok(rx) => {
                self.app.connection_status = ConnectionStatus::Connected;
                rx
            }
            Err(e) => {
                self.app.connection_status = ConnectionStatus::Error(e.to_string());
                let (_tx, rx) = tokio::sync::mpsc::channel(1);
                rx
            }
        };

        // Initial Data Fetch
        self.refresh_data().await?;

        loop {
            self.terminal.draw(|f| ui::render(&mut self.app, f))?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            tokio::select! {
                // Aegis Events
                Some(event) = event_rx.recv() => {
                    self.app.handle_aegis_event(event);
                }

                // Terminal Events
                res = tokio::task::spawn_blocking(move || event::poll(timeout)) => {
                    if let Ok(Ok(true)) = res {
                        if let Event::Key(key) = event::read()? {
                            match handle_key_events(key, &mut self.app) {
                                AppAction::Quit => break,
                                AppAction::SpawnAgent(task) => {
                                    let _ = self.client.send_command("agents.spawn", serde_json::json!({ "task": task })).await;
                                    self.refresh_data().await?;
                                }
                                AppAction::KillAgent(id) => {
                                    let _ = self.client.send_command("agents.kill", serde_json::json!({ "agent_id": id })).await;
                                    self.refresh_data().await?;
                                }
                                AppAction::SwitchProject(path) => {
                                    self.app.project_path = path;
                                    self.refresh_data().await?;
                                }
                                AppAction::None => {}
                            }
                        }
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }

        self.restore()?;
        Ok(())
    }

    async fn refresh_data(&mut self) -> Result<()> {
        if let Ok(agents) = self.client.send_command("agents.list", serde_json::Value::Null).await {
            if let Ok(agents_vec) = serde_json::from_value::<Vec<aegis_core::Agent>>(agents) {
                self.app.agents = agents_vec.into_iter().map(|a| (a.agent_id, a)).collect();
            }
        }

        if let Ok(tasks) = self.client.send_command("tasks.list", serde_json::Value::Null).await {
            if let Ok(tasks_vec) = serde_json::from_value::<Vec<aegis_core::Task>>(tasks) {
                self.app.tasks = tasks_vec;
            }
        }

        if let Ok(channels) = self.client.send_command("channels.list", serde_json::Value::Null).await {
            if let Ok(channels_vec) = serde_json::from_value::<Vec<aegis_core::ChannelRecord>>(channels) {
                self.app.channels = channels_vec;
            }
        }

        if let Ok(projects) = self.client.send_command("projects.list", serde_json::Value::Null).await {
            if let Ok(projects_vec) = serde_json::from_value::<Vec<crate::client::ProjectRecord>>(projects) {
                self.app.projects = projects_vec;
            }
        }

        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}
