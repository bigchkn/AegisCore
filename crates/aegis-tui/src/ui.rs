use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap, Clear},
    Frame,
};

use crate::app::{AppState, ConnectionStatus, PaneMode, Overlay};

pub fn render(app: &mut AppState, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Main Content
            Constraint::Length(3), // Footer / Status Bar
        ])
        .split(frame.area());

    render_header(app, frame, chunks[0]);
    render_main(app, frame, chunks[1]);
    render_footer(app, frame, chunks[2]);

    if app.overlay != Overlay::None {
        render_overlay(app, frame);
    }
}

fn render_header(app: &AppState, frame: &mut Frame, area: Rect) {
    let status_color = match app.connection_status {
        ConnectionStatus::Connected => Color::Green,
        ConnectionStatus::Connecting => Color::Yellow,
        ConnectionStatus::Disconnected => Color::Red,
        ConnectionStatus::Error(_) => Color::Magenta,
    };

    let title = Paragraph::new(format!(" AegisCore | Project: {} ", app.project_path.display()))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(status_color)))
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
    frame.render_widget(title, area);
}

fn render_main(app: &mut AppState, frame: &mut Frame, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Left: Agents & Channels
            Constraint::Percentage(50), // Middle: Logs or Terminal
            Constraint::Percentage(25), // Right: Tasks
        ])
        .split(area);

    render_left_sidebar(app, frame, main_chunks[0]);
    render_center_panel(app, frame, main_chunks[1]);
    render_right_sidebar(app, frame, main_chunks[2]);
}

fn render_left_sidebar(app: &AppState, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70), // Agents
            Constraint::Percentage(30), // Channels
        ])
        .split(area);

    // Agents
    let mut agents_vec: Vec<_> = app.agents.values().collect();
    agents_vec.sort_by_key(|a| a.name.clone());

    let agents: Vec<ListItem> = agents_vec
        .iter()
        .map(|a| {
            let style = if Some(a.agent_id) == app.selected_agent_id {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("[{:?}] {}", a.status, a.name)).style(style)
        })
        .collect();
    
    let agents_list = List::new(agents)
        .block(Block::default().borders(Borders::ALL).title(" Agents "));
    frame.render_widget(agents_list, chunks[0]);

    // Channels
    let channels: Vec<ListItem> = app.channels.iter()
        .map(|c| ListItem::new(format!("({:?}) {}", c.kind, c.name)))
        .collect();
    
    let channels_list = List::new(channels)
        .block(Block::default().borders(Borders::ALL).title(" Channels "));
    frame.render_widget(channels_list, chunks[1]);
}

fn render_center_panel(app: &mut AppState, frame: &mut Frame, area: Rect) {
    match app.mode {
        PaneMode::Input => {
            let terminal = Block::default().borders(Borders::ALL).title(" Terminal (INTERACTIVE) ");
            frame.render_widget(terminal, area);
        }
        _ => {
            let agent_logs = if let Some(id) = app.selected_agent_id {
                app.logs.get(&id).map(|l| l.join("\n")).unwrap_or_else(|| "No logs found for selected agent.".to_string())
            } else {
                "Select an agent to see logs.".to_string()
            };

            let logs_para = Paragraph::new(agent_logs)
                .block(Block::default().borders(Borders::ALL).title(" Logs "))
                .wrap(Wrap { trim: false });
            frame.render_widget(logs_para, area);
        }
    }
}

fn render_right_sidebar(app: &AppState, frame: &mut Frame, area: Rect) {
    let tasks: Vec<ListItem> = app.tasks.iter()
        .map(|t| ListItem::new(format!("[{:?}] {}", t.status, t.description)))
        .collect();
    
    let tasks_list = List::new(tasks)
        .block(Block::default().borders(Borders::ALL).title(" Tasks "));
    frame.render_widget(tasks_list, area);
}

fn render_footer(app: &AppState, frame: &mut Frame, area: Rect) {
    let mode_str = match app.mode {
        PaneMode::Normal => "NORMAL",
        PaneMode::Command => "COMMAND",
        PaneMode::Input => "INPUT (INTERACTIVE)",
    };

    let help_text = match app.mode {
        PaneMode::Normal => " [q]uit | [s]pawn | [x]kill | [?]help | [j/k] navigate ",
        PaneMode::Input => " [Esc] back to normal | Terminal interactive ",
        PaneMode::Command => " [Esc] cancel | [Enter] execute ",
    };

    let footer = Paragraph::new(format!(" MODE: {} | {}", mode_str, help_text))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, area);
}

fn render_overlay(app: &AppState, frame: &mut Frame) {
    let area = centered_rect(60, 40, frame.area());
    frame.render_widget(Clear, area);

    match &app.overlay {
        Overlay::Help => render_help_overlay(frame, area),
        Overlay::SpawnPrompt { input } => render_spawn_overlay(frame, area, input),
        Overlay::ConfirmKill { agent_id } => render_kill_overlay(frame, area, *agent_id, app),
        Overlay::None => {}
    }
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let help_text = r#"
  AegisCore TUI Help
  ──────────────────
  [q]       Quit
  [s]       Spawn new Splinter agent
  [x]       Kill selected agent
  [i]       Enter interactive terminal mode
  [j/k]     Navigate agent list
  [:]       Enter command mode
  [?]       This help screen
  
  [Esc]     Back to Normal mode / Close overlay
  "#;

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn render_spawn_overlay(frame: &mut Frame, area: Rect, input: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    let title = Paragraph::new(" Task Description for New Agent: ")
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(title, chunks[0]);

    let input_field = Paragraph::new(input)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
    frame.render_widget(input_field, chunks[1]);
}

fn render_kill_overlay(frame: &mut Frame, area: Rect, agent_id: uuid::Uuid, app: &AppState) {
    let agent_name = app.agents.get(&agent_id).map(|a| a.name.as_str()).unwrap_or("unknown");
    
    let text = format!("\n  Are you sure you want to KILL agent '{}'?\n\n  [y] Yes, Terminate  [n] No, Cancel", agent_name);
    let para = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Confirm Kill "))
        .style(Style::default().fg(Color::Red));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
