use crate::app::App;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use super::common;

/// Draw the server stderr logs tab.
pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    draw_server_selector(f, chunks[0], app);
    draw_stderr_panel(f, chunks[1], app);
}

fn draw_server_selector(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .connections
        .iter()
        .enumerate()
        .map(|(i, conn)| {
            let indicator = if i == app.selected { "▸ " } else { "  " };
            let stderr_count = conn.stderr_lines.len();
            let badge = if stderr_count > 0 {
                format!(" ({stderr_count})")
            } else {
                String::new()
            };
            let style = if i == app.selected {
                Style::default()
                    .bg(common::BG_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{indicator}{}{badge}", conn.config.name)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Servers ")
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );
    f.render_widget(list, area);
}

fn draw_stderr_panel(f: &mut Frame, area: Rect, app: &App) {
    if app.connections.is_empty() {
        let para = Paragraph::new("No servers configured.").block(
            Block::default()
                .title(" Server Logs ")
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        );
        f.render_widget(para, area);
        return;
    }

    let conn = &app.connections[app.selected];
    let title = format!(" Logs: {} ", conn.config.name);

    let max_lines = area.height.saturating_sub(2) as usize;
    let start = conn.stderr_lines.len().saturating_sub(max_lines);

    let lines: Vec<Line> = conn.stderr_lines.iter().skip(start).map(|line| {
        Line::from(Span::styled(
            line.as_str(),
            Style::default().fg(Color::DarkGray),
        ))
    }).collect();

    if lines.is_empty() {
        let para = Paragraph::new(Span::styled(
            "No stderr output captured.",
            Style::default().fg(Color::DarkGray),
        ))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        );
        f.render_widget(para, area);
    } else {
        let para = Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        );
        f.render_widget(para, area);
    }
}
