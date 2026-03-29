mod app;
mod checker;
mod config;
mod ui;

use app::{App, AppEvent};
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

    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, configs).await;

    // Restore terminal
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

    // Initial check
    app.spawn_checks(tx.clone());

    // Auto-refresh timer
    let tx_timer = tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let _ = tx_timer.send(AppEvent::RefreshAll);
        }
    });

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Handle pending events from checker tasks
        while let Ok(event) = rx.try_recv() {
            match &event {
                AppEvent::RefreshAll => {
                    app.spawn_checks(tx.clone());
                }
                _ => app.handle_event(event),
            }
        }

        if app.should_quit {
            break;
        }

        // Poll for keyboard input with timeout
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.handle_event(AppEvent::Quit);
                    }
                    KeyCode::Char('r') => {
                        app.spawn_checks(tx.clone());
                    }
                    KeyCode::Char('e') => {
                        if let Some(config_path) = app.selected_config_path().map(String::from) {
                            // Suspend TUI
                            disable_raw_mode()?;
                            io::stdout().execute(LeaveAlternateScreen)?;

                            // Open editor
                            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".into());
                            let _ = std::process::Command::new(&editor)
                                .arg(&config_path)
                                .status();

                            // Restore TUI
                            enable_raw_mode()?;
                            io::stdout().execute(EnterAlternateScreen)?;
                            terminal.clear()?;

                            // Reload and refresh
                            app.handle_event(AppEvent::ReloadConfig);
                            app.spawn_checks(tx.clone());
                        }
                    }
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
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
