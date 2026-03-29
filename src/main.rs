mod app;
mod chat;
mod chat_config;
mod chat_provider;
mod config;
mod connection;
mod inspector;
mod tokens;
mod ui;

use app::{App, AppEvent, Tab};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let configs = config::load_config().map_err(|e| {
        eprintln!("Error: {e}");
        e
    })?;

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, configs).await;

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    configs: Vec<config::ServerConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new(configs);
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    app.chat_tx = Some(tx.clone());

    app.connect_all(tx.clone());

    let tx_timer = tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let _ = tx_timer.send(AppEvent::HealthCheckAll);
        }
    });

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::HealthCheckAll => {
                    app.spawn_health_checks(tx.clone());
                }
                event => app.handle_event(event),
            }
        }

        if app.should_quit {
            break;
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Help overlay dismissal
                if app.show_help {
                    app.show_help = false;
                    continue;
                }

                // Search mode captures all keys
                if app.search_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.search_active = false;
                            app.search_query.clear();
                        }
                        KeyCode::Enter => {
                            app.search_active = false;
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }

                // Inspector input mode captures all keys
                if app.active_tab == Tab::Inspector && app.inspector.input_mode {
                    match key.code {
                        KeyCode::Esc => {
                            app.inspector.input_mode = false;
                        }
                        KeyCode::Enter => {
                            app.inspector.input_mode = false;
                            app.execute_selected_tool(tx.clone());
                        }
                        KeyCode::Backspace => {
                            app.inspector.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.inspector.input_buffer.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }

                // Chat input mode captures all keys
                if app.active_tab == Tab::Chat && app.chat.input_mode {
                    match key.code {
                        KeyCode::Esc => {
                            app.chat.input_mode = false;
                        }
                        KeyCode::Enter => {
                            app.chat.input_mode = false;
                            app.send_chat_message(tx.clone());
                        }
                        KeyCode::Backspace => {
                            app.chat.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.chat.input_buffer.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }

                // Normal key handling
                match key.code {
                    // Chat-specific keys (guarded, must come before unguarded)
                    KeyCode::Esc if app.active_tab == Tab::Chat && app.chat.is_streaming => {
                        app.chat.cancel_stream();
                    }
                    KeyCode::Char('i') if app.active_tab == Tab::Chat => {
                        if app.chat.error.is_some() {
                            app.chat.error = None;
                        }
                        app.chat.input_mode = true;
                    }
                    KeyCode::Char('p') if app.active_tab == Tab::Chat => {
                        app.chat.cycle_provider(&app.ai_config.clone());
                    }
                    KeyCode::Char('n') if app.active_tab == Tab::Chat => {
                        app.chat.new_conversation();
                    }
                    KeyCode::Tab if app.active_tab == Tab::Chat => {
                        if !app.connections.is_empty() {
                            app.chat.context_cursor =
                                (app.chat.context_cursor + 1) % app.connections.len();
                        }
                    }
                    KeyCode::Char(' ') if app.active_tab == Tab::Chat => {
                        let cursor = app.chat.context_cursor;
                        if cursor < app.connections.len() {
                            app.chat.toggle_server_context(cursor);
                        }
                    }
                    // Inspector-specific keys (guarded)
                    KeyCode::Char('i') if app.active_tab == Tab::Inspector => {
                        app.inspector.input_mode = true;
                    }
                    KeyCode::Enter if app.active_tab == Tab::Inspector => {
                        app.execute_selected_tool(tx.clone());
                    }
                    // Global keys
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.handle_event(AppEvent::Quit);
                    }
                    KeyCode::Char('/') => {
                        app.search_active = true;
                        app.search_query.clear();
                    }
                    KeyCode::Char('?') => {
                        app.show_help = true;
                    }
                    // Tab switching
                    KeyCode::Char('1') => {
                        app.handle_event(AppEvent::SetTab(Tab::Dashboard));
                    }
                    KeyCode::Char('2') => {
                        app.handle_event(AppEvent::SetTab(Tab::Inspector));
                    }
                    KeyCode::Char('3') => {
                        app.handle_event(AppEvent::SetTab(Tab::Protocol));
                    }
                    KeyCode::Char('4') => {
                        app.handle_event(AppEvent::SetTab(Tab::Logs));
                    }
                    KeyCode::Char('5') => {
                        app.handle_event(AppEvent::SetTab(Tab::Chat));
                    }
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.handle_event(AppEvent::Up);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.handle_event(AppEvent::Down);
                    }
                    KeyCode::Char('K') | KeyCode::PageUp => {
                        app.handle_event(AppEvent::ScrollUp);
                    }
                    KeyCode::Char('J') | KeyCode::PageDown => {
                        app.handle_event(AppEvent::ScrollDown);
                    }
                    // Tab-specific keys
                    KeyCode::Char('r') => {
                        app.refresh_all(tx.clone());
                    }
                    KeyCode::Char('c') => {
                        app.toggle_connection(tx.clone());
                    }
                    KeyCode::Tab => {
                        app.handle_event(AppEvent::CycleDetailTab);
                    }
                    KeyCode::Char('e') => {
                        if let Some(config_path) = app.selected_config_path().map(String::from) {
                            disable_raw_mode()?;
                            io::stdout().execute(LeaveAlternateScreen)?;

                            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
                            let _ = std::process::Command::new(&editor)
                                .arg(&config_path)
                                .status();

                            enable_raw_mode()?;
                            io::stdout().execute(EnterAlternateScreen)?;
                            terminal.clear()?;

                            app.handle_event(AppEvent::ReloadConfig);
                            app.connect_all(tx.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    for conn in &mut app.connections {
        conn.disconnect();
    }

    Ok(())
}
