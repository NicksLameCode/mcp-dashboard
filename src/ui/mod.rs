mod chat;
mod common;
mod dashboard;
mod inspector;
mod protocol;
mod server_logs;

use crate::app::{App, Tab};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &App) {
    let has_search = app.search_active || !app.search_query.is_empty();
    let constraints = if has_search {
        vec![
            Constraint::Length(3),  // title + tabs
            Constraint::Length(1),  // search bar
            Constraint::Min(10),   // main content
            Constraint::Length(8), // logs
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(8),
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    draw_header(f, chunks[0], app);

    if has_search {
        draw_search_bar(f, chunks[1], app);
        match app.active_tab {
            Tab::Dashboard => dashboard::draw(f, chunks[2], app),
            Tab::Inspector => inspector::draw(f, chunks[2], app),
            Tab::Protocol => protocol::draw(f, chunks[2], app),
            Tab::Logs => server_logs::draw(f, chunks[2], app),
            Tab::Chat => chat::draw(f, chunks[2], app),
        }
        draw_activity_log(f, chunks[3], app);
    } else {
        match app.active_tab {
            Tab::Dashboard => dashboard::draw(f, chunks[1], app),
            Tab::Inspector => inspector::draw(f, chunks[1], app),
            Tab::Protocol => protocol::draw(f, chunks[1], app),
            Tab::Logs => server_logs::draw(f, chunks[1], app),
            Tab::Chat => chat::draw(f, chunks[1], app),
        }
        draw_activity_log(f, chunks[2], app);
    }

    // Help overlay on top of everything
    if app.show_help {
        draw_help_overlay(f);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    let help_items = match app.active_tab {
        Tab::Dashboard => vec![
            ("j/k", "nav"),
            ("Tab", "detail"),
            ("/", "search"),
            ("r", "refresh"),
            ("c", "connect"),
            ("e", "edit"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Tab::Inspector => vec![
            ("j/k", "nav"),
            ("i", "input"),
            ("Enter", "exec"),
            ("?", "help"),
            ("q", "quit"),
        ],
        Tab::Protocol => vec![("j/k", "nav"), ("?", "help"), ("q", "quit")],
        Tab::Logs => vec![("j/k", "nav"), ("?", "help"), ("q", "quit")],
        Tab::Chat => vec![
            ("i", "input"),
            ("Enter", "send"),
            ("p", "provider"),
            ("n", "new"),
            ("Tab", "servers"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };

    let mut spans = vec![
        Span::styled(
            " MCP Dashboard ",
            Style::default()
                .fg(Color::White)
                .bg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    for (i, (key, action)) in help_items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(*key, Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(format!(" {action}")));
    }

    let title = Paragraph::new(Line::from(spans));
    f.render_widget(title, chunks[0]);

    let tabs = [
        (Tab::Dashboard, "1:Dashboard"),
        (Tab::Inspector, "2:Inspector"),
        (Tab::Protocol, "3:Protocol"),
        (Tab::Logs, "4:Logs"),
        (Tab::Chat, "5:Chat"),
    ];

    let tab_spans: Vec<Span> = tabs
        .iter()
        .flat_map(|(tab, label)| {
            let style = if *tab == app.active_tab {
                Style::default()
                    .fg(Color::White)
                    .bg(common::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            vec![Span::styled(format!(" {label} "), style), Span::raw(" ")]
        })
        .collect();

    let tab_line =
        Paragraph::new(Line::from(tab_spans)).block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(tab_line, chunks[1]);
}

fn draw_search_bar(f: &mut Frame, area: Rect, app: &App) {
    let cursor = if app.search_active { "▏" } else { "" };
    let filtered = app.filtered_indices().len();
    let total = app.connections.len();

    let line = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(&app.search_query),
        Span::raw(cursor),
        Span::raw("  "),
        Span::styled(
            format!("{filtered}/{total}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let style = if app.search_active {
        Style::default().bg(Color::Rgb(50, 50, 80))
    } else {
        Style::default()
    };

    let para = Paragraph::new(line).style(style);
    f.render_widget(para, area);
}

fn draw_activity_log(f: &mut Frame, area: Rect, app: &App) {
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
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("[{:<16}] ", entry.server),
                    Style::default().fg(common::ACCENT),
                ),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::raw(&entry.message),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(
        Block::default()
            .title(" Activity ")
            .borders(Borders::ALL)
            .border_style(common::border_style()),
    );

    f.render_widget(para, area);
}

fn draw_help_overlay(f: &mut Frame) {
    let area = f.area();
    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 40u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let help_text = vec![
        Line::from(Span::styled(
            "Global",
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  1/2/3/4    Switch tabs"),
        Line::from("  j/k ↑/↓   Navigate server list"),
        Line::from("  J/K PgDn   Scroll detail panel"),
        Line::from("  r          Refresh all / reconnect"),
        Line::from("  c          Connect/disconnect server"),
        Line::from("  e          Edit server config"),
        Line::from("  /          Search/filter servers"),
        Line::from("  ?          Toggle this help"),
        Line::from("  q / Esc    Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Dashboard (Tab 1)",
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Tab        Cycle: Tools/Resources/Prompts"),
        Line::from(""),
        Line::from(Span::styled(
            "Inspector (Tab 2)",
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  j/k        Navigate tool list"),
        Line::from("  i          Edit input parameters (JSON)"),
        Line::from("  Enter      Execute selected tool"),
        Line::from("  Esc        Exit input mode"),
        Line::from(""),
        Line::from(Span::styled(
            "Chat (Tab 5)",
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  i          Enter chat input mode"),
        Line::from("  Enter      Send message"),
        Line::from("  Esc        Exit input / cancel stream"),
        Line::from("  p          Cycle AI provider"),
        Line::from("  n          New conversation"),
        Line::from("  Tab        Cycle server context"),
        Line::from("  Space      Toggle server in context"),
        Line::from("  J/K PgDn   Scroll messages"),
        Line::from(""),
        Line::from(Span::styled(
            "Search Mode (/)",
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  Type       Filter servers by name"),
        Line::from("  Enter      Keep filter, exit search"),
        Line::from("  Esc        Clear filter, exit search"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let para = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(para, popup_area);
}
