use crate::app::App;
use crate::connection::ConnectionState;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table, Wrap};
use ratatui::Frame;

use super::common;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_server_list(f, chunks[0], app);
    draw_detail_panel(f, chunks[1], app);
}

fn draw_server_list(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Server").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Tools").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Tokens").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Source").style(Style::default().add_modifier(Modifier::BOLD)),
    ]);

    let filtered = app.filtered_indices();
    let rows: Vec<Row> = filtered
        .iter()
        .map(|&i| {
            let conn = &app.connections[i];
            let (status_text, status_color, tool_count) = match &conn.state {
                ConnectionState::Connected { .. } => (
                    "● OK".to_string(),
                    Color::Green,
                    format!("{}", conn.tools.len()),
                ),
                ConnectionState::Connecting => {
                    ("⟳ ...".to_string(), Color::Yellow, "-".to_string())
                }
                ConnectionState::Error(e) => {
                    let short = if e.len() > 20 {
                        format!("{}…", &e[..20])
                    } else {
                        e.clone()
                    };
                    (format!("✗ {short}"), Color::Red, "-".to_string())
                }
                ConnectionState::Disconnected => {
                    ("○ Off".to_string(), Color::DarkGray, "-".to_string())
                }
            };

            let style = if i == app.selected {
                Style::default()
                    .bg(common::BG_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let indicator = if i == app.selected { "▸ " } else { "  " };

            let token_est = crate::tokens::estimate(&conn.tools, &conn.resources, &conn.prompts);
            let token_color = token_est.severity_color();

            Row::new(vec![
                Cell::from(format!("{}{}", indicator, conn.config.name)),
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(tool_count),
                Cell::from(token_est.display()).style(Style::default().fg(token_color)),
                Cell::from(conn.config.source.label())
                    .style(Style::default().fg(Color::DarkGray)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Min(20),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Length(14),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" Servers ")
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );

    f.render_widget(table, area);
}

fn draw_detail_panel(f: &mut Frame, area: Rect, app: &App) {
    if app.connections.is_empty() {
        let para = Paragraph::new("No servers configured. Press 'e' to edit config.").block(
            Block::default()
                .title(" Details ")
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        );
        f.render_widget(para, area);
        return;
    }

    let conn = &app.connections[app.selected];
    let title = format!(" {} ", conn.config.name);

    // Split detail area: sparkline at top, content below
    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    // Response time sparkline
    if conn.is_connected() && !conn.response_history.is_empty() {
        let data: Vec<u64> = conn.response_history.iter().copied().collect();
        let sparkline = Sparkline::default()
            .data(&data)
            .max(data.iter().max().copied().unwrap_or(1000))
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .title(format!(
                        " Response ({}ms avg) ",
                        data.iter().sum::<u64>() / data.len() as u64
                    ))
                    .borders(Borders::ALL)
                    .border_style(common::border_style()),
            );
        f.render_widget(sparkline, detail_chunks[0]);
    } else {
        let placeholder = Block::default()
            .title(" Response ")
            .borders(Borders::ALL)
            .border_style(common::border_style());
        f.render_widget(placeholder, detail_chunks[0]);
    }

    let content_area = detail_chunks[1];

    let lines: Vec<Line> = match &conn.state {
        ConnectionState::Connected {
            server_name,
            connected_at,
        } => {
            let response_ms = conn.response_history.back().copied().unwrap_or(0);
            let uptime = chrono::Local::now()
                .signed_duration_since(*connected_at)
                .num_seconds();
            let uptime_str = if uptime >= 3600 {
                format!("{}h {}m", uptime / 3600, (uptime % 3600) / 60)
            } else if uptime >= 60 {
                format!("{}m {}s", uptime / 60, uptime % 60)
            } else {
                format!("{uptime}s")
            };

            let token_est = crate::tokens::estimate(&conn.tools, &conn.resources, &conn.prompts);

            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Server: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(server_name.as_deref().unwrap_or("unknown")),
                    Span::raw("  "),
                    Span::styled("Uptime: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(uptime_str),
                ]),
                Line::from(vec![
                    Span::styled("Response: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{response_ms}ms")),
                    Span::raw("  "),
                    Span::styled("Tokens: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        token_est.display(),
                        Style::default().fg(token_est.severity_color()),
                    ),
                    Span::styled(
                        format!(
                            " ({}T/{}R/{}P)",
                            token_est.tools, token_est.resources, token_est.prompts
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Tools: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}", conn.tools.len())),
                    Span::raw("  "),
                    Span::styled("Resources: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}", conn.resources.len())),
                    Span::raw("  "),
                    Span::styled("Prompts: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}", conn.prompts.len())),
                ]),
                Line::from(""),
            ];

            // Show tools, resources, or prompts based on detail_tab
            match app.detail_tab {
                crate::app::DetailTab::Tools => {
                    lines.push(Line::from(Span::styled(
                        "─── Tools ───",
                        Style::default()
                            .fg(common::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for tool in conn.tools.iter().skip(app.scroll_offset) {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {:<24}", tool.name),
                                Style::default()
                                    .fg(common::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                tool.description.as_deref().unwrap_or("").to_string(),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                    }
                }
                crate::app::DetailTab::Resources => {
                    lines.push(Line::from(Span::styled(
                        "─── Resources ───",
                        Style::default()
                            .fg(common::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for resource in conn.resources.iter().skip(app.scroll_offset) {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {:<24}", resource.name),
                                Style::default()
                                    .fg(common::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                resource.uri.as_str(),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                        if let Some(desc) = &resource.description {
                            lines.push(Line::from(Span::styled(
                                format!("  {:<24}{desc}", ""),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
                crate::app::DetailTab::Prompts => {
                    lines.push(Line::from(Span::styled(
                        "─── Prompts ───",
                        Style::default()
                            .fg(common::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for prompt in conn.prompts.iter().skip(app.scroll_offset) {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {:<24}", prompt.name),
                                Style::default()
                                    .fg(common::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                prompt.description.as_deref().unwrap_or("").to_string(),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]));
                        if let Some(args) = &prompt.arguments {
                            for arg in args {
                                let required = if arg.required.unwrap_or(false) {
                                    " *"
                                } else {
                                    ""
                                };
                                lines.push(Line::from(Span::styled(
                                    format!(
                                        "    {}{required}",
                                        arg.name
                                    ),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                }
            }
            lines
        }
        ConnectionState::Connecting => {
            vec![Line::from(Span::styled(
                "Connecting...",
                Style::default().fg(Color::Yellow),
            ))]
        }
        ConnectionState::Error(e) => {
            vec![
                Line::from(Span::styled(
                    "✗ Error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(e.as_str()),
                Line::from(""),
                Line::from(Span::styled(
                    "Press 'c' to reconnect",
                    Style::default().fg(Color::DarkGray),
                )),
            ]
        }
        ConnectionState::Disconnected => {
            vec![Line::from(Span::styled(
                "Press 'c' to connect",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(para, content_area);
}
