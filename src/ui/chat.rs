use crate::app::App;
use crate::chat::MessageRole;
use crate::tokens;
use crate::ui::common;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // status + context bar
            Constraint::Min(8),   // message history
            Constraint::Length(4), // input area
        ])
        .split(area);

    draw_status_bar(f, chunks[0], app);
    draw_messages(f, chunks[1], app);
    draw_input(f, chunks[2], app);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // Row 1: Provider / model / tokens
    let token_display = if app.chat.total_input_tokens > 0 || app.chat.total_output_tokens > 0 {
        format!(
            " | Tokens: {} in / {} out",
            format_tokens(app.chat.total_input_tokens),
            format_tokens(app.chat.total_output_tokens)
        )
    } else {
        String::new()
    };

    let status_line = Line::from(vec![
        Span::styled(" Provider: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.chat.provider.label(),
            Style::default()
                .fg(common::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | Model: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&app.chat.model, Style::default().fg(Color::White)),
        Span::styled(token_display, Style::default().fg(Color::DarkGray)),
        if app.chat.is_streaming {
            Span::styled(
                " | Streaming...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
    ]);
    f.render_widget(Paragraph::new(status_line), rows[0]);

    // Row 2: Server context chips
    let mut context_spans = vec![Span::styled(
        " Context: ",
        Style::default().fg(Color::DarkGray),
    )];

    if app.connections.is_empty() {
        context_spans.push(Span::styled(
            "(no servers)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for (i, conn) in app.connections.iter().enumerate() {
            let none_toggled = app.chat.context_server_indices.is_empty();
            let is_selected = none_toggled || app.chat.context_server_indices.contains(&i);
            let is_cursor = app.chat.context_cursor == i;

            let icon = if is_selected { "+" } else { "o" };
            let style = if is_cursor {
                Style::default()
                    .fg(if is_selected {
                        Color::Green
                    } else {
                        Color::White
                    })
                    .bg(common::BG_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            context_spans.push(Span::styled(
                format!("{icon}{} ", conn.config.name),
                style,
            ));
        }
        context_spans.push(Span::styled(
            " (Tab=cycle Space=toggle)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Show estimated context tokens
    if !app.connections.is_empty() {
        let active_indices: Vec<usize> = if app.chat.context_server_indices.is_empty() {
            (0..app.connections.len()).collect()
        } else {
            app.chat.context_server_indices.clone()
        };
        let total_tokens: usize = active_indices
            .iter()
            .filter_map(|&idx| app.connections.get(idx))
            .map(|c| tokens::estimate(&c.tools, &c.resources, &c.prompts).total)
            .sum();
        context_spans.push(Span::styled(
            format!(" ~{}tok", format_tokens(total_tokens)),
            Style::default().fg(Color::DarkGray),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(context_spans)), rows[1]);
}

fn draw_messages(f: &mut Frame, area: Rect, app: &App) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;

    // Build all display lines from messages
    let mut lines: Vec<Line> = Vec::new();

    if app.chat.messages.is_empty() && !app.chat.is_streaming {
        lines.push(Line::from(Span::styled(
            "Press 'i' to start typing, then Enter to send.",
            Style::default().fg(Color::DarkGray),
        )));
        if app.chat.context_server_indices.is_empty() && !app.connections.is_empty() {
            lines.push(Line::from(Span::styled(
                "All servers included by default. Use Tab/Space to select specific ones.",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    for msg in &app.chat.messages {
        lines.push(Line::from("")); // spacing between messages
        match msg.role {
            MessageRole::User => {
                let wrapped = wrap_text(&msg.content, inner_width.saturating_sub(5));
                for (i, line) in wrapped.into_iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "You: ",
                                Style::default()
                                    .fg(common::CHAT_USER)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(line),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("     "),
                            Span::raw(line),
                        ]));
                    }
                }
            }
            MessageRole::Assistant => {
                let wrapped = wrap_text(&msg.content, inner_width.saturating_sub(4));
                for (i, line) in wrapped.into_iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "AI: ",
                                Style::default()
                                    .fg(common::CHAT_ASSISTANT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(line),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::raw(line),
                        ]));
                    }
                }
            }
            MessageRole::ToolCall => {
                let info = msg.tool_call.as_ref();
                let server = info
                    .map(|t| t.server_name.as_str())
                    .unwrap_or("?");
                lines.push(Line::from(vec![
                    Span::styled(
                        ">> Tool Call ",
                        Style::default()
                            .fg(common::CHAT_TOOL_CALL)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{server}] "),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        msg.content.clone(),
                        Style::default().fg(common::CHAT_TOOL_CALL),
                    ),
                ]));
            }
            MessageRole::ToolResult => {
                let wrapped = wrap_text(&msg.content, inner_width.saturating_sub(4));
                for (i, line) in wrapped.into_iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                " <- ",
                                Style::default().fg(common::CHAT_TOOL_RESULT),
                            ),
                            Span::styled(line, Style::default().fg(common::CHAT_TOOL_RESULT)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(line, Style::default().fg(common::CHAT_TOOL_RESULT)),
                        ]));
                    }
                }
            }
            MessageRole::System => {
                lines.push(Line::from(Span::styled(
                    msg.content.clone(),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }
    }

    // Streaming buffer (in-progress response)
    if app.chat.is_streaming && !app.chat.streaming_buffer.is_empty() {
        lines.push(Line::from(""));
        let content = format!("{}\u{2588}", app.chat.streaming_buffer); // block cursor
        let wrapped = wrap_text(&content, inner_width.saturating_sub(4));
        for (i, line) in wrapped.into_iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        "AI: ",
                        Style::default()
                            .fg(common::CHAT_ASSISTANT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(line, Style::default().fg(common::CHAT_ASSISTANT)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(line, Style::default().fg(common::CHAT_ASSISTANT)),
                ]));
            }
        }
    } else if app.chat.is_streaming {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "AI: ",
                Style::default()
                    .fg(common::CHAT_ASSISTANT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(common::CHAT_ASSISTANT)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]));
    }

    // Error display
    if let Some(ref err) = app.chat.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Error: {err}"),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Scrolling: auto-scroll to bottom unless user has scrolled up
    let total_lines = lines.len();
    let scroll = if app.chat.scroll_offset == 0 {
        // Auto-scroll to bottom
        total_lines.saturating_sub(inner_height)
    } else {
        total_lines.saturating_sub(inner_height).saturating_sub(app.chat.scroll_offset)
    };

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Chat ")
                .borders(Borders::ALL)
                .border_style(common::border_style()),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    f.render_widget(para, area);
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(1)])
        .split(area);

    let cursor = if app.chat.input_mode { "\u{2581}" } else { "" };
    let input_text = format!("{}{}", app.chat.input_buffer, cursor);

    let border_style = if app.chat.input_mode {
        Style::default().fg(Color::Yellow)
    } else {
        common::border_style()
    };

    let bg = if app.chat.input_mode {
        Style::default().bg(Color::Rgb(50, 50, 80))
    } else {
        Style::default()
    };

    let input = Paragraph::new(input_text)
        .block(
            Block::default()
                .title(if app.chat.input_mode {
                    " Input (Enter=send, Esc=cancel) "
                } else {
                    " Input (i=type) "
                })
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .style(bg)
        .wrap(Wrap { trim: false });

    f.render_widget(input, chunks[0]);

    // Hint line
    let hints = Line::from(vec![
        Span::styled(" i", Style::default().fg(Color::Yellow)),
        Span::raw(" input  "),
        Span::styled("p", Style::default().fg(Color::Yellow)),
        Span::raw(" provider  "),
        Span::styled("m", Style::default().fg(Color::Yellow)),
        Span::raw(" model  "),
        Span::styled("n", Style::default().fg(Color::Yellow)),
        Span::raw(" new chat  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(" servers  "),
        Span::styled("Space", Style::default().fg(Color::Yellow)),
        Span::raw(" toggle  "),
        Span::styled("J/K", Style::default().fg(Color::Yellow)),
        Span::raw(" scroll"),
    ]);
    f.render_widget(Paragraph::new(hints), chunks[1]);
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        // Use char count for proper Unicode handling
        if line.chars().count() <= width {
            lines.push(line.to_string());
        } else {
            let mut remaining = line;
            while remaining.chars().count() > width {
                // Find a char-safe split point
                let byte_at_width: usize = remaining
                    .char_indices()
                    .nth(width)
                    .map(|(i, _)| i)
                    .unwrap_or(remaining.len());
                let split = remaining[..byte_at_width]
                    .rfind(' ')
                    .unwrap_or(byte_at_width);
                lines.push(remaining[..split].to_string());
                remaining = remaining[split..].trim_start();
            }
            if !remaining.is_empty() {
                lines.push(remaining.to_string());
            }
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn format_tokens(count: usize) -> String {
    if count >= 1000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        format!("{count}")
    }
}
