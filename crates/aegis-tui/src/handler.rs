use crate::app::{AppState, Overlay, PaneMode};
use crate::client::ProjectRecord;
use crossterm::event::{KeyCode, KeyEvent};
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub enum AppAction {
    Quit,
    SpawnAgent(String),
    KillAgent(Uuid),
    AnswerClarification(Uuid, String),
    SwitchProject(std::path::PathBuf),
    None,
}

pub fn handle_key_events(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    if app.overlay != Overlay::None {
        return handle_overlay(key_event, app);
    }

    match app.mode {
        PaneMode::Normal => handle_normal_mode(key_event, app),
        PaneMode::Input => handle_input_mode(key_event, app),
        PaneMode::Command => handle_command_mode(key_event, app),
    }
}

fn handle_overlay(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    let mut overlay = std::mem::replace(&mut app.overlay, Overlay::None);
    let action = match &mut overlay {
        Overlay::Help => AppAction::None,
        Overlay::ProjectSwitcher {
            projects,
            selected_idx,
        } => handle_project_switcher(key_event, projects, selected_idx),
        Overlay::SpawnPrompt { input } => handle_spawn_prompt(key_event, input),
        Overlay::ConfirmKill { agent_id } => handle_confirm_kill(key_event, *agent_id),
        Overlay::Clarification { request, input } => {
            handle_clarification(key_event, request, input)
        }
        Overlay::AttachError { .. } => AppAction::None,
        Overlay::None => AppAction::None,
    };

    if action == AppAction::None && overlay != Overlay::None && key_event.code != KeyCode::Esc {
        app.overlay = overlay;
    }

    action
}

fn handle_project_switcher(
    key_event: KeyEvent,
    projects: &[ProjectRecord],
    selected_idx: &mut usize,
) -> AppAction {
    match key_event.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if !projects.is_empty() {
                *selected_idx = (*selected_idx + projects.len() - 1) % projects.len();
            }
            AppAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !projects.is_empty() {
                *selected_idx = (*selected_idx + 1) % projects.len();
            }
            AppAction::None
        }
        KeyCode::Enter => {
            if let Some(project) = projects.get(*selected_idx) {
                AppAction::SwitchProject(project.root_path.clone())
            } else {
                AppAction::None
            }
        }
        KeyCode::Esc => AppAction::None,
        _ => AppAction::None,
    }
}

fn handle_spawn_prompt(key_event: KeyEvent, input: &mut String) -> AppAction {
    match key_event.code {
        KeyCode::Enter => AppAction::SpawnAgent(input.clone()),
        KeyCode::Esc => AppAction::None,
        KeyCode::Char(c) => {
            input.push(c);
            AppAction::None
        }
        KeyCode::Backspace => {
            input.pop();
            AppAction::None
        }
        _ => AppAction::None,
    }
}

fn handle_confirm_kill(key_event: KeyEvent, agent_id: Uuid) -> AppAction {
    match key_event.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => AppAction::KillAgent(agent_id),
        _ => AppAction::None,
    }
}

fn handle_clarification(
    key_event: KeyEvent,
    request: &crate::client::ClarificationRequest,
    input: &mut String,
) -> AppAction {
    match key_event.code {
        KeyCode::Enter => AppAction::AnswerClarification(request.request_id, input.clone()),
        KeyCode::Esc => AppAction::None,
        KeyCode::Char(c) => {
            input.push(c);
            AppAction::None
        }
        KeyCode::Backspace => {
            input.pop();
            AppAction::None
        }
        _ => AppAction::None,
    }
}

fn handle_normal_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Char('q') => AppAction::Quit,
        KeyCode::Char('?') | KeyCode::Char('h') => {
            app.overlay = Overlay::Help;
            AppAction::None
        }
        KeyCode::Char('p') => {
            app.overlay = Overlay::ProjectSwitcher {
                projects: app.projects.clone(),
                selected_idx: 0,
            };
            AppAction::None
        }
        KeyCode::Char('s') => {
            app.overlay = Overlay::SpawnPrompt {
                input: String::new(),
            };
            AppAction::None
        }
        KeyCode::Char('x') => {
            if let Some(id) = app.selected_agent_id {
                app.overlay = Overlay::ConfirmKill { agent_id: id };
            }
            AppAction::None
        }
        KeyCode::Char('i') | KeyCode::Enter => {
            if let Some(id) = app.selected_agent_id {
                app.attached_agent_id = Some(id);
                app.attached_interactive = true;
                app.mode = PaneMode::Input;
            }
            AppAction::None
        }
        KeyCode::Char('r') => {
            if let Some(id) = app.selected_agent_id {
                if let Some(req) = app
                    .pending_clarifications
                    .iter()
                    .find(|r| r.agent_id == id && r.status == "open")
                {
                    app.overlay = Overlay::Clarification {
                        request: req.clone(),
                        input: String::new(),
                    };
                }
            }
            AppAction::None
        }
        KeyCode::Char(':') => {
            app.mode = PaneMode::Command;
            AppAction::None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            navigate_agents(app, 1);
            AppAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            navigate_agents(app, -1);
            AppAction::None
        }
        _ => AppAction::None,
    }
}

fn navigate_agents(app: &mut AppState, delta: i32) {
    let mut ids: Vec<Uuid> = app.agents.keys().cloned().collect();
    if ids.is_empty() {
        return;
    }
    ids.sort();

    let current_idx = app
        .selected_agent_id
        .and_then(|id| ids.iter().position(|&x| x == id))
        .unwrap_or(0) as i32;

    let next_idx = (current_idx + delta).rem_euclid(ids.len() as i32) as usize;
    app.selected_agent_id = Some(ids[next_idx]);
}

fn handle_input_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Esc => {
            app.mode = PaneMode::Normal;
            app.attached_interactive = false;
            AppAction::None
        }
        _ => AppAction::None,
    }
}

fn handle_command_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Esc | KeyCode::Enter => {
            app.mode = PaneMode::Normal;
            AppAction::None
        }
        _ => AppAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_normal_mode_navigation() {
        let mut app = AppState::new(std::path::PathBuf::from("/tmp"));
        let agent_id = Uuid::new_v4();
        app.agents.insert(
            agent_id,
            aegis_core::Agent {
                agent_id,
                name: "test-agent".to_string(),
                kind: aegis_core::AgentKind::Bastion,
                status: aegis_core::AgentStatus::Active,
                role: "worker".to_string(),
                parent_id: None,
                task_id: None,
                tmux_session: "aegis".to_string(),
                tmux_window: 0,
                tmux_pane: "%0".to_string(),
                worktree_path: std::path::PathBuf::from("/tmp"),
                cli_provider: "claude-code".to_string(),
                fallback_cascade: vec![],
                sandbox_profile: std::path::PathBuf::from("/tmp/sandbox"),
                log_path: std::path::PathBuf::from("/tmp/log"),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                terminated_at: None,
            },
        );

        assert_eq!(
            handle_key_events(key(KeyCode::Char('j')), &mut app),
            AppAction::None
        );
        assert!(app.selected_agent_id.is_some());
    }

    #[test]
    fn test_mode_switching() {
        let mut app = AppState::new(std::path::PathBuf::from("/tmp"));
        let agent_id = Uuid::new_v4();
        app.selected_agent_id = Some(agent_id);

        handle_key_events(key(KeyCode::Char('i')), &mut app);
        assert_eq!(app.mode, PaneMode::Input);
        assert!(app.attached_interactive);
        assert_eq!(app.attached_agent_id, Some(agent_id));

        handle_key_events(key(KeyCode::Esc), &mut app);
        assert_eq!(app.mode, PaneMode::Normal);
        assert!(!app.attached_interactive);

        handle_key_events(key(KeyCode::Char(':')), &mut app);
        assert_eq!(app.mode, PaneMode::Command);
    }

    #[test]
    fn test_attach_selection_marks_target() {
        let mut app = AppState::new(std::path::PathBuf::from("/tmp"));
        let agent_id = Uuid::new_v4();
        app.selected_agent_id = Some(agent_id);

        handle_key_events(key(KeyCode::Enter), &mut app);

        assert_eq!(app.mode, PaneMode::Input);
        assert_eq!(app.attached_agent_id, Some(agent_id));
        assert!(app.attached_interactive);
    }

    #[test]
    fn test_overlay_navigation() {
        let mut app = AppState::new(std::path::PathBuf::from("/tmp"));
        app.overlay = Overlay::Help;

        assert_eq!(
            handle_key_events(key(KeyCode::Esc), &mut app),
            AppAction::None
        );
        assert_eq!(app.overlay, Overlay::None);
    }
}
