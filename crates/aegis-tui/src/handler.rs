use crate::app::{AppState, PaneMode, Overlay};
use crossterm::event::{KeyCode, KeyEvent};
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub enum AppAction {
    Quit,
    SpawnAgent(String),
    KillAgent(Uuid),
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
        Overlay::Help => {
            AppAction::None
        }
        Overlay::SpawnPrompt { input } => handle_spawn_prompt(key_event, input),
        Overlay::ConfirmKill { agent_id } => handle_confirm_kill(key_event, *agent_id),
        Overlay::None => AppAction::None,
    };

    // If no action was taken (meaning overlay wasn't closed by Enter/Action),
    // and it's not Esc, put it back.
    if action == AppAction::None && overlay != Overlay::None && key_event.code != KeyCode::Esc {
        app.overlay = overlay;
    }

    action
}

fn handle_spawn_prompt(key_event: KeyEvent, input: &mut String) -> AppAction {
    match key_event.code {
        KeyCode::Enter => {
            AppAction::SpawnAgent(input.clone())
        }
        KeyCode::Esc => {
            AppAction::None
        }
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
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            AppAction::KillAgent(agent_id)
        }
        _ => {
            AppAction::None
        }
    }
}

fn handle_normal_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Char('q') => AppAction::Quit,
        KeyCode::Char('?') | KeyCode::Char('h') => {
            app.overlay = Overlay::Help;
            AppAction::None
        }
        KeyCode::Char('s') => {
            app.overlay = Overlay::SpawnPrompt { input: String::new() };
            AppAction::None
        }
        KeyCode::Char('x') => {
            if let Some(id) = app.selected_agent_id {
                app.overlay = Overlay::ConfirmKill { agent_id: id };
            }
            AppAction::None
        }
        KeyCode::Char('i') => {
            app.mode = PaneMode::Input;
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
    if ids.is_empty() { return; }
    ids.sort(); // Consistent order

    let current_idx = app.selected_agent_id
        .and_then(|id| ids.iter().position(|&x| x == id))
        .unwrap_or(0) as i32;
    
    let next_idx = (current_idx + delta).rem_euclid(ids.len() as i32) as usize;
    app.selected_agent_id = Some(ids[next_idx]);
}

fn handle_input_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Esc => {
            app.mode = PaneMode::Normal;
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
