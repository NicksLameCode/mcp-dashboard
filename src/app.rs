use crate::config::ServerConfig;
use crate::connection::{
    spawn_connect, spawn_health_check, spawn_refresh_capabilities, ConnectionState, LogEntry,
    ManagedConnection,
};
use crate::inspector::{InspectorState, ProtocolEntry};
use chrono::Local;
use rmcp::model::{Prompt, Resource, Tool};
use rmcp::{Peer, RoleClient};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Inspector,
    Protocol,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Tools,
    Resources,
    Prompts,
}

pub struct App {
    pub connections: Vec<ManagedConnection>,
    pub selected: usize,
    pub logs: Vec<LogEntry>,
    pub should_quit: bool,
    pub scroll_offset: usize,
    pub active_tab: Tab,
    pub detail_tab: DetailTab,
    pub inspector: InspectorState,
    pub protocol_log: Vec<ProtocolEntry>,
    pub search_active: bool,
    pub search_query: String,
    pub show_help: bool,
}

pub enum AppEvent {
    // Connection lifecycle
    ConnectionEstablished(usize, Peer<RoleClient>, Option<String>),
    ConnectionFailed(usize, String),
    ConnectionLost(usize, String),
    CapabilitiesLoaded(usize, Vec<Tool>, Vec<Resource>, Vec<Prompt>, u64),
    StderrLine(usize, String),
    HealthCheckResult(usize, Result<(Vec<Tool>, u64), String>),
    // Tool execution
    ToolResult(usize, Result<(String, u64, bool), String>), // idx, Ok((text, ms, is_error)) | Err
    // Timer
    HealthCheckAll,
    // User actions
    ReloadConfig,
    SetTab(Tab),
    CycleDetailTab,
    Quit,
    Up,
    Down,
    ScrollUp,
    ScrollDown,
}

impl App {
    pub fn new(configs: Vec<ServerConfig>) -> Self {
        let connections = configs.into_iter().map(ManagedConnection::new).collect();
        Self {
            connections,
            selected: 0,
            logs: Vec::new(),
            should_quit: false,
            scroll_offset: 0,
            active_tab: Tab::Dashboard,
            detail_tab: DetailTab::Tools,
            inspector: InspectorState::default(),
            protocol_log: Vec::new(),
            search_active: false,
            search_query: String::new(),
            show_help: false,
        }
    }

    /// Get indices of connections matching the search filter.
    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.search_query.is_empty() {
            (0..self.connections.len()).collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.connections
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.config.name.to_lowercase().contains(&query)
                        || c.config.source.label().contains(&query)
                })
                .map(|(i, _)| i)
                .collect()
        }
    }

    pub fn selected_config_path(&self) -> Option<&str> {
        self.connections
            .get(self.selected)
            .and_then(|c| c.config.config_path.as_deref())
    }

    pub fn connect_all(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        for (idx, conn) in self.connections.iter_mut().enumerate() {
            conn.state = ConnectionState::Connecting;
            let shutdown_tx = spawn_connect(idx, &conn.config, tx.clone());
            conn.shutdown_tx = Some(shutdown_tx);
        }
    }

    pub fn toggle_connection(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        let idx = self.selected;
        if let Some(conn) = self.connections.get_mut(idx) {
            if conn.is_connected() || matches!(conn.state, ConnectionState::Connecting) {
                let name = conn.config.name.clone();
                conn.disconnect();
                self.add_log(&name, "Disconnected", false);
            } else {
                let name = conn.config.name.clone();
                conn.state = ConnectionState::Connecting;
                let shutdown_tx = spawn_connect(idx, &conn.config, tx.clone());
                conn.shutdown_tx = Some(shutdown_tx);
                self.add_log(&name, "Connecting...", false);
            }
        }
    }

    pub fn refresh_all(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        for (idx, conn) in self.connections.iter_mut().enumerate() {
            if conn.is_connected() {
                if let Some(peer) = conn.peer.as_ref() {
                    spawn_refresh_capabilities(idx, peer, &tx);
                }
            } else if !matches!(conn.state, ConnectionState::Connecting) {
                conn.state = ConnectionState::Connecting;
                let shutdown_tx = spawn_connect(idx, &conn.config, tx.clone());
                conn.shutdown_tx = Some(shutdown_tx);
            }
        }
    }

    pub fn spawn_health_checks(&self, tx: mpsc::UnboundedSender<AppEvent>) {
        for (idx, conn) in self.connections.iter().enumerate() {
            if let Some(peer) = conn.peer.as_ref() {
                spawn_health_check(idx, peer, &tx);
            }
        }
    }

    pub fn execute_selected_tool(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        let idx = self.selected;
        let tool_idx = self.inspector.selected_tool;

        let (peer, tool_name, server_name) = match self.connections.get(idx) {
            Some(conn) => {
                let peer = match conn.peer.clone() {
                    Some(p) => p,
                    None => return,
                };
                let tool_name = match conn.tools.get(tool_idx) {
                    Some(t) => t.name.to_string(),
                    None => return,
                };
                (peer, tool_name, conn.config.name.clone())
            }
            None => return,
        };

        self.inspector.is_executing = true;
        self.inspector.result_lines.clear();
        self.inspector.result_scroll = 0;

        self.add_protocol_entry(
            &server_name,
            "tools/call",
            "→",
            &format!("call_tool({tool_name})"),
            None,
            false,
        );

        crate::inspector::spawn_execute_tool(
            idx,
            &peer,
            &tool_name,
            &self.inspector.input_buffer,
            &tx,
        );
    }

    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::ConnectionEstablished(idx, peer, server_name) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    let log_name = conn.config.name.clone();
                    let display = server_name
                        .as_deref()
                        .unwrap_or("unknown")
                        .to_string();
                    conn.state = ConnectionState::Connected {
                        server_name,
                        connected_at: Local::now(),
                    };
                    conn.peer = Some(peer);
                    self.add_log(&log_name, &format!("Connected ({display})"), false);
                    self.add_protocol_entry(
                        &log_name, "initialize", "←",
                        &format!("OK ({display})"), None, false,
                    );
                }
            }
            AppEvent::ConnectionFailed(idx, error) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    let log_name = conn.config.name.clone();
                    conn.state = ConnectionState::Error(error.clone());
                    conn.peer = None;
                    conn.shutdown_tx = None;
                    self.add_log(&log_name, &format!("Failed: {error}"), true);
                    self.add_protocol_entry(
                        &log_name, "initialize", "←",
                        &format!("Error: {error}"), None, true,
                    );
                }
            }
            AppEvent::ConnectionLost(idx, reason) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    let log_name = conn.config.name.clone();
                    conn.state = ConnectionState::Error(reason.clone());
                    conn.peer = None;
                    conn.shutdown_tx = None;
                    self.add_log(&log_name, &format!("Lost: {reason}"), true);
                }
            }
            AppEvent::CapabilitiesLoaded(idx, tools, resources, prompts, elapsed_ms) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    let log_name = conn.config.name.clone();
                    let tool_count = tools.len();
                    let resource_count = resources.len();
                    let prompt_count = prompts.len();
                    conn.tools = tools;
                    conn.resources = resources;
                    conn.prompts = prompts;
                    conn.record_response_time(elapsed_ms);
                    conn.last_check = Some(Local::now());
                    self.add_log(
                        &log_name,
                        &format!(
                            "{tool_count} tools, {resource_count} resources, {prompt_count} prompts ({elapsed_ms}ms)",
                        ),
                        false,
                    );
                    self.add_protocol_entry(
                        &log_name, "capabilities", "←",
                        &format!("{tool_count}T {resource_count}R {prompt_count}P"),
                        Some(elapsed_ms), false,
                    );
                }
            }
            AppEvent::StderrLine(idx, line) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    conn.add_stderr_line(line);
                }
            }
            AppEvent::HealthCheckResult(idx, result) => {
                if let Some(conn) = self.connections.get_mut(idx) {
                    match result {
                        Ok((tools, elapsed_ms)) => {
                            conn.tools = tools;
                            conn.record_response_time(elapsed_ms);
                            conn.last_check = Some(Local::now());
                        }
                        Err(e) => {
                            let log_name = conn.config.name.clone();
                            conn.state = ConnectionState::Error(e.clone());
                            conn.peer = None;
                            conn.shutdown_tx = None;
                            self.add_log(&log_name, &format!("Health check: {e}"), true);
                        }
                    }
                }
            }
            AppEvent::ToolResult(idx, result) => {
                self.inspector.is_executing = false;
                let server_name = self
                    .connections
                    .get(idx)
                    .map(|c| c.config.name.clone())
                    .unwrap_or_default();

                match result {
                    Ok((text, duration_ms, is_error)) => {
                        self.inspector.result_lines =
                            text.lines().map(String::from).collect();
                        self.inspector.result_is_error = is_error;
                        self.add_log(
                            &server_name,
                            &format!("Tool result ({duration_ms}ms)"),
                            is_error,
                        );
                        self.add_protocol_entry(
                            &server_name, "tools/call", "←",
                            &if is_error { "Error result".into() } else { format!("OK ({duration_ms}ms)") },
                            Some(duration_ms), is_error,
                        );
                    }
                    Err(e) => {
                        self.inspector.result_lines = vec![e.clone()];
                        self.inspector.result_is_error = true;
                        self.add_log(&server_name, &format!("Tool error: {e}"), true);
                        self.add_protocol_entry(
                            &server_name, "tools/call", "←",
                            &format!("Error: {e}"), None, true,
                        );
                    }
                }
            }
            AppEvent::Quit => self.should_quit = true,
            AppEvent::Up => {
                if self.active_tab == Tab::Inspector {
                    if self.inspector.selected_tool > 0 {
                        self.inspector.selected_tool -= 1;
                        self.inspector.result_lines.clear();
                        self.inspector.input_buffer.clear();
                    }
                } else if self.selected > 0 {
                    self.selected -= 1;
                    self.scroll_offset = 0;
                }
            }
            AppEvent::Down => {
                if self.active_tab == Tab::Inspector {
                    let max = self
                        .connections
                        .get(self.selected)
                        .map(|c| c.tools.len().saturating_sub(1))
                        .unwrap_or(0);
                    if self.inspector.selected_tool < max {
                        self.inspector.selected_tool += 1;
                        self.inspector.result_lines.clear();
                        self.inspector.input_buffer.clear();
                    }
                } else if self.selected < self.connections.len().saturating_sub(1) {
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
            AppEvent::ReloadConfig => {
                self.reload_config();
            }
            AppEvent::SetTab(tab) => {
                self.active_tab = tab;
                self.scroll_offset = 0;
            }
            AppEvent::CycleDetailTab => {
                self.detail_tab = match self.detail_tab {
                    DetailTab::Tools => DetailTab::Resources,
                    DetailTab::Resources => DetailTab::Prompts,
                    DetailTab::Prompts => DetailTab::Tools,
                };
                self.scroll_offset = 0;
            }
            AppEvent::HealthCheckAll => {}
        }
    }

    fn add_log(&mut self, server: &str, message: &str, is_error: bool) {
        self.logs.push(LogEntry {
            timestamp: Local::now(),
            server: server.to_string(),
            message: message.to_string(),
            is_error,
        });
        if self.logs.len() > 200 {
            self.logs.drain(0..self.logs.len() - 200);
        }
    }

    fn add_protocol_entry(
        &mut self,
        server: &str,
        method: &str,
        direction: &'static str,
        summary: &str,
        duration_ms: Option<u64>,
        is_error: bool,
    ) {
        self.protocol_log.push(ProtocolEntry {
            timestamp: Local::now(),
            server: server.to_string(),
            direction,
            method: method.to_string(),
            summary: summary.to_string(),
            duration_ms,
            is_error,
        });
        if self.protocol_log.len() > 500 {
            self.protocol_log.drain(0..self.protocol_log.len() - 500);
        }
    }

    fn reload_config(&mut self) {
        for conn in &mut self.connections {
            conn.disconnect();
        }
        if let Ok(configs) = crate::config::load_config() {
            self.connections = configs.into_iter().map(ManagedConnection::new).collect();
            if self.selected >= self.connections.len() {
                self.selected = self.connections.len().saturating_sub(1);
            }
        }
    }
}
