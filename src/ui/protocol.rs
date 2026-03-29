use crate::app::App;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::common;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let max_lines = area.height.saturating_sub(2) as usize;
    let start = app.protocol_log.len().saturating_sub(max_lines);

    let lines: Vec<Line> = app.protocol_log[start..]
        .iter()
        .map(|entry| {
            let time = entry.timestamp.format("%H:%M:%S").to_string();
            let color = if entry.is_error {
                Color::Red
            } else {
                Color::Green
            };
            let duration = entry
                .duration_ms
                .map(|ms| format!(" {ms}ms"))
                .unwrap_or_default();

            Line::from(vec![
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ", entry.direction),
                    Style::default()
                        .fg(if entry.direction == "→" {
                            Color::Cyan
                        } else {
                            Color::Yellow
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{:<16}] ", entry.server),
                    Style::default().fg(common::ACCENT),
                ),
                Span::styled(
                    format!("{:<14} ", entry.method),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(entry.summary.as_str(), Style::default().fg(color)),
                Span::styled(duration, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let title = format!(" Protocol ({} entries) ", app.protocol_log.len());
    let para = Paragraph::new(lines).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );

    f.render_widget(para, area);
}
