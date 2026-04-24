use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppState, ConnectionStatus, PaneMode};

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
    let agents: Vec<ListItem> = app.agents.values()
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
            // Interactive Terminal
            // For now, a placeholder until we wire the backend relay
            let terminal = Block::default().borders(Borders::ALL).title(" Terminal (INTERACTIVE) ");
            frame.render_widget(terminal, area);
        }
        _ => {
            // Logs
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
        PaneMode::Normal => " [q]uit | [i]nput | [:]command | [j/k] navigate ",
        PaneMode::Input => " [Esc] back to normal | Terminal interactive ",
        PaneMode::Command => " [Esc] cancel | [Enter] execute ",
    };

    let footer = Paragraph::new(format!(" MODE: {} | {}", mode_str, help_text))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, area);
}
