use crate::app::{AppState, PaneMode};
use crossterm::event::{KeyCode, KeyEvent};

pub enum AppAction {
    Quit,
    None,
}

pub fn handle_key_events(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match app.mode {
        PaneMode::Normal => handle_normal_mode(key_event, app),
        PaneMode::Input => handle_input_mode(key_event, app),
        PaneMode::Command => handle_command_mode(key_event, app),
    }
}

fn handle_normal_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Char('q') => AppAction::Quit,
        KeyCode::Char('i') => {
            app.mode = PaneMode::Input;
            AppAction::None
        }
        KeyCode::Char(':') => {
            app.mode = PaneMode::Command;
            AppAction::None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            // Select next agent
            AppAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            // Select previous agent
            AppAction::None
        }
        _ => AppAction::None,
    }
}

fn handle_input_mode(key_event: KeyEvent, app: &mut AppState) -> AppAction {
    match key_event.code {
        KeyCode::Esc => {
            app.mode = PaneMode::Normal;
            AppAction::None
        }
        _ => {
            // Forward keys to PaneRelay (to be implemented in UI loop)
            AppAction::None
        }
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
