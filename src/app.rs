use crate::checker::{check_server, LogEntry, ServerState, ServerStatus};
use crate::config::ServerConfig;
use tokio::sync::mpsc;

pub struct App {
    pub servers: Vec<ServerState>,
    pub selected: usize,
    pub logs: Vec<LogEntry>,
    pub checking: bool,
    pub should_quit: bool,
    pub scroll_offset: usize,
}

pub enum AppEvent {
    CheckComplete(usize, ServerStatus, LogEntry),
    RefreshAll,
    ReloadConfig,
    Quit,
    Up,
    Down,
    ScrollUp,
    ScrollDown,
}

impl App {
    pub fn selected_config_path(&self) -> Option<&str> {
        self.servers
            .get(self.selected)
            .and_then(|s| s.config.config_path.as_deref())
    }

    pub fn reload_config(&mut self) {
        if let Ok(configs) = crate::config::load_config() {
            self.servers = configs
                .into_iter()
                .map(|config| ServerState {
                    config,
                    status: ServerStatus::Unknown,
                    last_check: None,
                })
                .collect();
            if self.selected >= self.servers.len() {
                self.selected = self.servers.len().saturating_sub(1);
            }
        }
    }
}

impl App {
    #[allow(clippy::new_without_default)]
    pub fn new(configs: Vec<ServerConfig>) -> Self {
        let servers = configs
            .into_iter()
            .map(|config| ServerState {
                config,
                status: ServerStatus::Unknown,
                last_check: None,
            })
            .collect();

        Self {
            servers,
            selected: 0,
            logs: Vec::new(),
            checking: false,
            should_quit: false,
            scroll_offset: 0,
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::CheckComplete(idx, status, log) => {
                if let Some(server) = self.servers.get_mut(idx) {
                    server.status = status;
                    server.last_check = Some(chrono::Local::now());
                }
                self.logs.push(log);
                // Keep last 200 logs
                if self.logs.len() > 200 {
                    self.logs.drain(0..self.logs.len() - 200);
                }
            }
            AppEvent::Quit => self.should_quit = true,
            AppEvent::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.scroll_offset = 0;
                }
            }
            AppEvent::Down => {
                if self.selected < self.servers.len().saturating_sub(1) {
                    self.selected += 1;
                    self.scroll_offset = 0;
                }
            }
            AppEvent::ScrollUp => {
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            AppEvent::ScrollDown => {
                self.scroll_offset += 1;
            }
            AppEvent::RefreshAll => {}
            AppEvent::ReloadConfig => {
                self.reload_config();
            }
        }
    }

    pub fn spawn_checks(&self, tx: mpsc::UnboundedSender<AppEvent>) {
        for (idx, server) in self.servers.iter().enumerate() {
            let config = server.config.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let (status, log) = check_server(&config).await;
                let _ = tx.send(AppEvent::CheckComplete(idx, status, log));
            });
        }
    }
}
