use crate::app::App;
use crate::connection::ConnectionState;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::common;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    draw_tool_list(f, chunks[0], app);
    draw_tool_detail(f, chunks[1], app);
}

fn draw_tool_list(f: &mut Frame, area: Rect, app: &App) {
    let conn = match app.connections.get(app.selected) {
        Some(c) => c,
        None => {
            let para = Paragraph::new("No server selected.").block(
                Block::default()
                    .title(" Tools ")
                    .borders(Borders::ALL)
                    .border_style(common::border_style()),
            );
            f.render_widget(para, area);
            return;
        }
    };

    if !conn.is_connected() {
        let msg = match &conn.state {
            ConnectionState::Connecting => "Connecting...",
            ConnectionState::Error(_) => "Server error. Press 'c' to reconnect.",
            ConnectionState::Disconnected => "Press 'c' to connect.",
            _ => "",
        };
        let para = Paragraph::new(msg).block(
            Block::default()
                .title(format!(" {} ", conn.config.name))
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        );
        f.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = conn
        .tools
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let indicator = if i == app.inspector.selected_tool {
                "▸ "
            } else {
                "  "
            };
            let style = if i == app.inspector.selected_tool {
                Style::default()
                    .bg(common::BG_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!("{indicator}{}", tool.name)).style(style)
        })
        .collect();

    let title = format!(" {} ({} tools) ", conn.config.name, conn.tools.len());
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );
    f.render_widget(list, area);
}

fn draw_tool_detail(f: &mut Frame, area: Rect, app: &App) {
    let conn = match app.connections.get(app.selected) {
        Some(c) if c.is_connected() => c,
        _ => {
            let para = Paragraph::new("Select a connected server.").block(
                Block::default()
                    .title(" Detail ")
                    .borders(Borders::ALL)
                    .border_style(common::border_style()),
            );
            f.render_widget(para, area);
            return;
        }
    };

    let tool = match conn.tools.get(app.inspector.selected_tool) {
        Some(t) => t,
        None => {
            let para = Paragraph::new("No tools available.").block(
                Block::default()
                    .title(" Detail ")
                    .borders(Borders::ALL)
                    .border_style(common::border_style()),
            );
            f.render_widget(para, area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // tool info + schema hint
            Constraint::Length(3), // input
            Constraint::Min(5),   // result
        ])
        .split(area);

    // Tool info
    let mut info_lines = vec![
        Line::from(vec![
            Span::styled("Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                tool.name.to_string(),
                Style::default()
                    .fg(common::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Description: ", Style::default().fg(Color::DarkGray)),
            Span::raw(tool.description.as_deref().unwrap_or("(none)")),
        ]),
    ];

    // Show schema keys as hints
    if let Some(props) = tool.input_schema.get("properties") {
        if let Some(obj) = props.as_object() {
            let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
            info_lines.push(Line::from(vec![
                Span::styled("Params: ", Style::default().fg(Color::DarkGray)),
                Span::raw(keys.join(", ")),
            ]));
        }
    }

    let info = Paragraph::new(info_lines).block(
        Block::default()
            .title(format!(" {} ", tool.name))
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );
    f.render_widget(info, chunks[0]);

    // Input field
    let input_style = if app.inspector.input_mode {
        Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 80))
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let input_title = if app.inspector.input_mode {
        " Input (Esc=exit, Enter=execute) "
    } else {
        " Input (i=edit, Enter=execute) "
    };

    let display_text = if app.inspector.input_buffer.is_empty() && !app.inspector.input_mode {
        "{}"
    } else {
        &app.inspector.input_buffer
    };

    let cursor = if app.inspector.input_mode { "▏" } else { "" };

    let input = Paragraph::new(format!("{display_text}{cursor}"))
        .style(input_style)
        .block(
            Block::default()
                .title(input_title)
                .borders(Borders::ALL)
                .border_style(if app.inspector.input_mode {
                    Style::default().fg(Color::Yellow)
                } else {
                    common::border_style()
                }),
        );
    f.render_widget(input, chunks[1]);

    // Result area
    let result_title = if app.inspector.is_executing {
        " Result (executing...) ".to_string()
    } else {
        " Result ".to_string()
    };

    let result_color = if app.inspector.result_is_error {
        Color::Red
    } else {
        Color::Green
    };

    let result_lines: Vec<Line> = if app.inspector.result_lines.is_empty() {
        vec![Line::from(Span::styled(
            if app.inspector.is_executing {
                "Executing..."
            } else {
                "Press Enter to execute the selected tool."
            },
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.inspector
            .result_lines
            .iter()
            .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(result_color))))
            .collect()
    };

    let result = Paragraph::new(result_lines)
        .block(
            Block::default()
                .title(result_title)
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(result, chunks[2]);
}
