use crate::app::App;
use crate::checker::ServerStatus;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title
            Constraint::Min(10),   // main content
            Constraint::Length(8), // logs
        ])
        .split(f.area());

    draw_title(f, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    draw_server_list(f, main_chunks[0], app);
    draw_tool_details(f, main_chunks[1], app);
    draw_logs(f, chunks[2], app);
}

fn draw_title(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " MCP Server Dashboard ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(38, 139, 210))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(" refresh  "),
        Span::styled("e", Style::default().fg(Color::Yellow)),
        Span::raw(" edit config  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(title, area);
}

fn draw_server_list(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec![
        Cell::from("Server").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Tools").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Type").style(Style::default().add_modifier(Modifier::BOLD)),
    ]);

    let rows: Vec<Row> = app
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let (status_text, status_color, tool_count) = match &s.status {
                ServerStatus::Healthy { tools, .. } => {
                    ("● OK".to_string(), Color::Green, format!("{}", tools.len()))
                }
                ServerStatus::Error(e) => {
                    let short = if e.len() > 20 {
                        format!("{}…", &e[..20])
                    } else {
                        e.clone()
                    };
                    (format!("✗ {short}"), Color::Red, "-".to_string())
                }
                ServerStatus::Unknown => ("? ---".to_string(), Color::DarkGray, "-".to_string()),
            };

            let style = if i == app.selected {
                Style::default()
                    .bg(Color::Rgb(238, 232, 213))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let indicator = if i == app.selected { "▸ " } else { "  " };

            Row::new(vec![
                Cell::from(format!("{}{}", indicator, s.config.name)),
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(tool_count),
                Cell::from(s.config.server_type.clone())
                    .style(Style::default().fg(Color::DarkGray)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Min(22),
            Constraint::Length(5),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" Servers ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(38, 139, 210))),
    );

    f.render_widget(table, area);
}

fn draw_tool_details(f: &mut Frame, area: Rect, app: &App) {
    let server = &app.servers[app.selected];
    let title = format!(" Tools ({}) ", server.config.name);

    let lines: Vec<Line> = match &server.status {
        ServerStatus::Healthy {
            tools,
            server_name,
            response_ms,
        } => {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Server: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(server_name.as_deref().unwrap_or("unknown")),
                ]),
                Line::from(vec![
                    Span::styled("Response: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{response_ms}ms")),
                ]),
                Line::from(""),
            ];

            for tool in tools.iter().skip(app.scroll_offset) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:<24}", tool.name),
                        Style::default()
                            .fg(Color::Rgb(38, 139, 210))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        &tool.description,
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            lines
        }
        ServerStatus::Error(e) => {
            vec![
                Line::from(Span::styled(
                    "✗ Error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(e.as_str()),
            ]
        }
        ServerStatus::Unknown => {
            vec![Line::from(Span::styled(
                "Press 'r' to check servers",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Rgb(38, 139, 210))),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(para, area);
}

fn draw_logs(f: &mut Frame, area: Rect, app: &App) {
    let max_lines = area.height.saturating_sub(2) as usize;
    let start = app.logs.len().saturating_sub(max_lines);

    let lines: Vec<Line> = app.logs[start..]
        .iter()
        .map(|entry| {
            let time = entry.timestamp.format("%H:%M:%S").to_string();
            let color = if entry.is_error {
                Color::Red
            } else {
                Color::Green
            };
            let icon = if entry.is_error { "✗" } else { "●" };

            Line::from(vec![
                Span::styled(
                    format!("{time} "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{:<16}] ", entry.server),
                    Style::default().fg(Color::Rgb(38, 139, 210)),
                ),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .title(" Logs ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(38, 139, 210))),
    );

    f.render_widget(para, area);
}
