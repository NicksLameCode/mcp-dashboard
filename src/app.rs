use crate::chat::ChatState;
use crate::chat_config::AiConfig;
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
    Chat,
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
    pub chat: ChatState,
    pub ai_config: AiConfig,
    pub chat_tx: Option<mpsc::UnboundedSender<AppEvent>>,
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
    // Chat events
    ChatToken(String),
    ChatResponseComplete {
        input_tokens: usize,
        output_tokens: usize,
    },
    ChatError(String),
    ChatToolCall {
        id: String,
        name: String,
        server_idx: usize,
        args: serde_json::Value,
    },
    ChatToolResult {
        id: String,
        result: String,
        is_error: bool,
        duration_ms: u64,
    },
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
        let ai_config = crate::chat_config::load_ai_config();
        let chat = ChatState::new(&ai_config);
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
            chat,
            ai_config,
            chat_tx: None,
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

    pub fn send_chat_message(&mut self, tx: mpsc::UnboundedSender<AppEvent>) {
        let input = self.chat.input_buffer.trim().to_string();
        if input.is_empty() || self.chat.is_streaming {
            return;
        }

        // Add user message to history
        self.chat.messages.push(crate::chat::ChatMessage {
            role: crate::chat::MessageRole::User,
            content: input.clone(),
            timestamp: Local::now(),
            tool_call: None,
        });
        self.chat.input_buffer.clear();
        self.chat.scroll_offset = 0; // auto-scroll to bottom

        // Build context — default to all connected servers if none toggled
        let indices = if self.chat.context_server_indices.is_empty() {
            (0..self.connections.len()).collect()
        } else {
            self.chat.context_server_indices.clone()
        };

        let system_prompt = crate::chat::build_system_prompt(&self.connections, &indices);

        // Spawn the chat request
        self.chat.is_streaming = true;
        self.chat.streaming_buffer.clear();

        let messages = self.chat.messages.clone();
        let ai_config = self.ai_config.clone();
        crate::chat_provider::spawn_chat_request(
            self.chat.provider,
            &ai_config,
            &messages,
            &system_prompt,
            &self.connections,
            &indices,
            tx,
            &mut self.chat,
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
            AppEvent::ChatToken(text) => {
                self.chat.streaming_buffer.push_str(&text);
            }
            AppEvent::ChatResponseComplete {
                input_tokens,
                output_tokens,
            } => {
                self.chat.is_streaming = false;
                self.chat.streaming_handle = None;
                let content = std::mem::take(&mut self.chat.streaming_buffer);
                if !content.is_empty() {
                    self.chat.messages.push(crate::chat::ChatMessage {
                        role: crate::chat::MessageRole::Assistant,
                        content,
                        timestamp: Local::now(),
                        tool_call: None,
                    });
                }
                self.chat.total_input_tokens += input_tokens;
                self.chat.total_output_tokens += output_tokens;
                self.chat.error = None;
                self.chat.trim_history(100);
            }
            AppEvent::ChatError(err) => {
                self.chat.is_streaming = false;
                self.chat.streaming_handle = None;
                self.chat.streaming_buffer.clear();
                self.chat.error = Some(err);
            }
            AppEvent::ChatToolCall {
                id,
                name,
                server_idx,
                args,
            } => {
                // Display tool call in chat
                let server_name = self
                    .connections
                    .get(server_idx)
                    .map(|c| c.config.name.clone())
                    .unwrap_or_else(|| "unknown".into());

                // Finalize any streaming text before the tool call
                if !self.chat.streaming_buffer.is_empty() {
                    let content = std::mem::take(&mut self.chat.streaming_buffer);
                    self.chat.messages.push(crate::chat::ChatMessage {
                        role: crate::chat::MessageRole::Assistant,
                        content,
                        timestamp: Local::now(),
                        tool_call: None,
                    });
                }

                let args_display = serde_json::to_string(&args).unwrap_or_default();
                self.chat.messages.push(crate::chat::ChatMessage {
                    role: crate::chat::MessageRole::ToolCall,
                    content: format!("{name}({args_display})"),
                    timestamp: Local::now(),
                    tool_call: Some(crate::chat::ToolCallInfo {
                        tool_name: name.clone(),
                        server_name: server_name.clone(),
                        is_result: false,
                    }),
                });

                self.add_protocol_entry(
                    &server_name,
                    "tools/call",
                    "\u{2192}",
                    &format!("chat: call_tool({name})"),
                    None,
                    false,
                );

                // Store pending tool call for execution
                self.chat
                    .pending_tool_calls
                    .push(crate::chat::PendingToolCall {
                        id: id.clone(),
                        tool_name: name.clone(),
                        server_index: server_idx,
                        arguments: args.clone(),
                    });

                // Execute the tool via MCP — resolve original name
                let original_name = if name.starts_with('s') && name.contains('_') {
                    name.split_once('_').map(|x| x.1).unwrap_or(&name).to_string()
                } else {
                    name.clone()
                };

                if let Some(conn) = self.connections.get(server_idx) {
                    if let Some(peer) = conn.peer.clone() {
                        let tx_clone = match self.chat_tx.clone() {
                            Some(tx) => tx,
                            None => return,
                        };
                        let id_clone = id;
                        tokio::spawn(async move {
                            let start = std::time::Instant::now();
                            let arguments = args.as_object().cloned();
                            let params = if let Some(args) = arguments {
                                rmcp::model::CallToolRequestParams::new(original_name)
                                    .with_arguments(args)
                            } else {
                                rmcp::model::CallToolRequestParams::new(original_name)
                            };

                            let result = tokio::time::timeout(
                                std::time::Duration::from_secs(30),
                                peer.call_tool(params),
                            )
                            .await;

                            let elapsed = start.elapsed().as_millis() as u64;

                            match result {
                                Ok(Ok(call_result)) => {
                                    let is_error = call_result.is_error.unwrap_or(false);
                                    let mut output = Vec::new();
                                    for content in &call_result.content {
                                        if let Some(text) = content.raw.as_text() {
                                            output.push(text.text.clone());
                                        } else {
                                            output.push("[non-text content]".to_string());
                                        }
                                    }
                                    let result_text = if output.is_empty() {
                                        "(empty result)".to_string()
                                    } else {
                                        output.join("\n")
                                    };
                                    let _ = tx_clone.send(AppEvent::ChatToolResult {
                                        id: id_clone,
                                        result: result_text,
                                        is_error,
                                        duration_ms: elapsed,
                                    });
                                }
                                Ok(Err(e)) => {
                                    let _ = tx_clone.send(AppEvent::ChatToolResult {
                                        id: id_clone,
                                        result: format!("Tool error: {e}"),
                                        is_error: true,
                                        duration_ms: elapsed,
                                    });
                                }
                                Err(_) => {
                                    let _ = tx_clone.send(AppEvent::ChatToolResult {
                                        id: id_clone,
                                        result: "Tool call timed out (30s)".to_string(),
                                        is_error: true,
                                        duration_ms: 30000,
                                    });
                                }
                            }
                        });
                    }
                }
            }
            AppEvent::ChatToolResult {
                id,
                result,
                is_error,
                duration_ms,
            } => {
                let server_name = self
                    .chat
                    .pending_tool_calls
                    .last()
                    .and_then(|tc| {
                        self.connections
                            .get(tc.server_index)
                            .map(|c| c.config.name.clone())
                    })
                    .unwrap_or_default();

                self.chat.messages.push(crate::chat::ChatMessage {
                    role: crate::chat::MessageRole::ToolResult,
                    content: result.clone(),
                    timestamp: Local::now(),
                    tool_call: Some(crate::chat::ToolCallInfo {
                        tool_name: id.clone(), // Store the tool_use_id for Anthropic tool_result
                        server_name: server_name.clone(),
                        is_result: true,
                    }),
                });

                self.add_protocol_entry(
                    &server_name,
                    "tools/call",
                    "\u{2190}",
                    &if is_error {
                        "chat: Error result".into()
                    } else {
                        format!("chat: OK ({duration_ms}ms)")
                    },
                    Some(duration_ms),
                    is_error,
                );

                // Remove the completed tool call
                self.chat.pending_tool_calls.pop();

                // Re-invoke the AI with the tool result (agentic loop)
                if let Some(tx) = &self.chat_tx {
                    let tx = tx.clone();
                    self.chat.is_streaming = true;
                    self.chat.streaming_buffer.clear();
                    let messages = self.chat.messages.clone();
                    let ai_config = self.ai_config.clone();
                    let indices = if self.chat.context_server_indices.is_empty() {
                        (0..self.connections.len()).collect()
                    } else {
                        self.chat.context_server_indices.clone()
                    };
                    let system_prompt =
                        crate::chat::build_system_prompt(&self.connections, &indices);

                    crate::chat_provider::spawn_chat_request(
                        self.chat.provider,
                        &ai_config,
                        &messages,
                        &system_prompt,
                        &self.connections,
                        &indices,
                        tx,
                        &mut self.chat,
                    );
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
                if self.active_tab == Tab::Chat {
                    self.chat.scroll_offset += 1; // scroll up = increase offset from bottom
                } else if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            AppEvent::ScrollDown => {
                if self.active_tab == Tab::Chat {
                    if self.chat.scroll_offset > 0 {
                        self.chat.scroll_offset -= 1; // scroll down = decrease offset (toward bottom)
                    }
                } else {
                    self.scroll_offset += 1;
                }
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
