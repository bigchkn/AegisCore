use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc::error::TryRecvError;
use uuid::Uuid;

use crate::app::{AppState, ConnectionStatus, Overlay, PaneMode};
use crate::client::AegisClient;
use crate::handler::{handle_key_events, AppAction};
use crate::ui;

pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    pub app: AppState,
    client: AegisClient,
    attach_target: Option<Uuid>,
    attach_input_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    attach_output_rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
}

impl Tui {
    pub fn new(app: AppState, client: AegisClient) -> Result<Self> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            app,
            client,
            attach_target: None,
            attach_input_tx: None,
            attach_output_rx: None,
        })
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
            self.drain_attach_output();
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
                            let was_input = self.app.mode == PaneMode::Input;
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
                                AppAction::AnswerClarification(request_id, answer) => {
                                    let _ = self.client.clarify_answer(request_id, answer, serde_json::Value::Null).await;
                                    self.refresh_data().await?;
                                }
                                AppAction::SwitchProject(path) => {
                                    self.app.project_path = path;
                                    self.refresh_data().await?;
                                }
                                AppAction::None => {}
                            }

                            if was_input {
                                if self.app.mode != PaneMode::Input {
                                    self.detach_attach_session();
                                } else if self.attach_target.is_none() {
                                    self.ensure_attach_session().await?;
                                } else {
                                    self.forward_attach_input(key).await?;
                                }
                            } else if self.app.mode == PaneMode::Input {
                                self.ensure_attach_session().await?;
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
        let is_initial = self.app.agents.is_empty();

        if let Ok(agents) = self
            .client
            .send_command("agents.list", serde_json::Value::Null)
            .await
        {
            if let Ok(agents_vec) = serde_json::from_value::<Vec<aegis_core::Agent>>(agents) {
                self.app.agents = agents_vec.into_iter().map(|a| (a.agent_id, a)).collect();
            }
        }

        if let Ok(tasks) = self
            .client
            .send_command("tasks.list", serde_json::Value::Null)
            .await
        {
            if let Ok(tasks_vec) = serde_json::from_value::<Vec<aegis_core::Task>>(tasks) {
                self.app.tasks = tasks_vec;
            }
        }

        if let Ok(channels) = self
            .client
            .send_command("channels.list", serde_json::Value::Null)
            .await
        {
            if let Ok(channels_vec) =
                serde_json::from_value::<Vec<aegis_core::ChannelRecord>>(channels)
            {
                self.app.channels = channels_vec;
            }
        }

        if let Ok(projects) = self
            .client
            .send_command("projects.list", serde_json::Value::Null)
            .await
        {
            if let Ok(projects_vec) =
                serde_json::from_value::<Vec<crate::client::ProjectRecord>>(projects)
            {
                self.app.projects = projects_vec.clone();

                // If this is the initial data fetch, check if we should auto-attach
                if is_initial {
                    if let Some(project) = projects_vec
                        .into_iter()
                        .find(|p| p.root_path == self.app.project_path)
                    {
                        if let Some(agent_id) = project.last_attached_agent_id {
                            // Verify agent is alive and in our current agent list
                            if let Some(agent) = self.app.agents.get(&agent_id) {
                                if !agent.status.is_terminal() {
                                    self.app.selected_agent_id = Some(agent_id);
                                    self.app.attached_agent_id = Some(agent_id);
                                    // Trigger attach sequence
                                    let _ = self.ensure_attach_session().await;
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(clarifications) = self.client.clarify_list(None).await {
            self.app.pending_clarifications = clarifications;
        }

        if let Some(target) = self.attach_target {
            let target_missing_or_terminal = self
                .app
                .agents
                .get(&target)
                .map(|agent| agent.status.is_terminal())
                .unwrap_or(true);
            if target_missing_or_terminal {
                self.detach_attach_session();
                self.app.overlay = Overlay::AttachError {
                    agent_id: target,
                    message: "Attached agent is no longer available.".to_string(),
                };
            }
        }

        Ok(())
    }

    async fn ensure_attach_session(&mut self) -> Result<()> {
        if self.attach_target.is_some() {
            return Ok(());
        }

        let Some(agent_id) = self.app.attached_agent_id.or(self.app.selected_agent_id) else {
            return Ok(());
        };

        match self.client.attach_pane(agent_id).await {
            Ok((input_tx, output_rx)) => {
                self.attach_target = Some(agent_id);
                self.attach_input_tx = Some(input_tx);
                self.attach_output_rx = Some(output_rx);
                self.app.attached_agent_id = Some(agent_id);
                self.app.attached_interactive = true;
                self.app.attached_output.clear();
                self.app.overlay = Overlay::None;
                self.app.mode = PaneMode::Input;
            }
            Err(error) => {
                self.detach_attach_session();
                self.app.mode = PaneMode::Normal;
                self.app.overlay = Overlay::AttachError {
                    agent_id,
                    message: error.to_string(),
                };
            }
        }

        Ok(())
    }

    fn detach_attach_session(&mut self) {
        self.attach_target = None;
        self.attach_input_tx = None;
        self.attach_output_rx = None;
        self.app.attached_agent_id = None;
        self.app.attached_interactive = false;
    }

    fn drain_attach_output(&mut self) {
        let Some(rx) = self.attach_output_rx.as_mut() else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    if !text.is_empty() {
                        self.app.attached_output.push(text);
                        if self.app.attached_output.len() > 200 {
                            self.app.attached_output.remove(0);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let agent_id = self.attach_target.unwrap_or_else(Uuid::nil);
                    self.detach_attach_session();
                    self.app.mode = PaneMode::Normal;
                    self.app.overlay = Overlay::AttachError {
                        agent_id,
                        message: "Live pane stream disconnected. Reattach from the agent list."
                            .to_string(),
                    };
                    break;
                }
            }
        }
    }

    async fn forward_attach_input(&mut self, key: KeyEvent) -> Result<()> {
        let Some(tx) = self.attach_input_tx.as_ref() else {
            return Ok(());
        };

        if let Some(bytes) = key_event_to_bytes(key) {
            tx.send(bytes)
                .await
                .map_err(|error| anyhow::anyhow!("Failed to forward attach input: {error}"))?;
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

fn key_event_to_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Esc => None,
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                control_code(c).map(|code| vec![code])
            } else {
                Some(c.to_string().into_bytes())
            }
        }
        _ => None,
    }
}

fn control_code(c: char) -> Option<u8> {
    let upper = c.to_ascii_uppercase() as u8;
    match upper {
        b'@' => Some(0x00),
        b'A'..=b'Z' => Some(upper - b'@'),
        b'[' => Some(0x1b),
        b'\\' => Some(0x1c),
        b']' => Some(0x1d),
        b'^' => Some(0x1e),
        b'_' => Some(0x1f),
        _ => None,
    }
}
