use crate::app::AppEvent;
use crate::config::{ServerConfig, TransportType};
use chrono::{DateTime, Local};
use rmcp::model::{Prompt, Resource, Tool};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{Peer, RoleClient, ServiceExt};
use std::collections::VecDeque;
use std::time::Instant;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

const MAX_STDERR_LINES: usize = 500;
const MAX_RESPONSE_HISTORY: usize = 60;
const CONNECT_TIMEOUT_SECS: u64 = 15;
const CAPABILITY_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected {
        server_name: Option<String>,
        connected_at: DateTime<Local>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub server: String,
    pub message: String,
    pub is_error: bool,
}

pub struct ManagedConnection {
    pub config: ServerConfig,
    pub state: ConnectionState,
    pub peer: Option<Peer<RoleClient>>,
    pub shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub stderr_lines: VecDeque<String>,
    pub response_history: VecDeque<u64>,
    pub last_check: Option<DateTime<Local>>,
    pub tools: Vec<Tool>,
    pub resources: Vec<Resource>,
    pub prompts: Vec<Prompt>,
}

impl ManagedConnection {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            state: ConnectionState::Disconnected,
            peer: None,
            shutdown_tx: None,
            stderr_lines: VecDeque::new(),
            response_history: VecDeque::with_capacity(MAX_RESPONSE_HISTORY),
            last_check: None,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, ConnectionState::Connected { .. })
    }

    pub fn disconnect(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.peer = None;
        self.state = ConnectionState::Disconnected;
    }

    pub fn record_response_time(&mut self, ms: u64) {
        if self.response_history.len() >= MAX_RESPONSE_HISTORY {
            self.response_history.pop_front();
        }
        self.response_history.push_back(ms);
    }

    pub fn add_stderr_line(&mut self, line: String) {
        if self.stderr_lines.len() >= MAX_STDERR_LINES {
            self.stderr_lines.pop_front();
        }
        self.stderr_lines.push_back(line);
    }
}

/// Spawn a persistent connection task for a server. Returns a shutdown sender.
pub fn spawn_connect(
    idx: usize,
    config: &ServerConfig,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> tokio::sync::oneshot::Sender<()> {
    let config = config.clone();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        match config.transport {
            TransportType::Stdio => spawn_stdio_connection(idx, &config, &tx, &mut shutdown_rx).await,
            TransportType::Http => spawn_http_connection(idx, &config, &tx, &mut shutdown_rx).await,
        }
    });

    shutdown_tx
}

async fn spawn_stdio_connection(
    idx: usize,
    config: &ServerConfig,
    tx: &mpsc::UnboundedSender<AppEvent>,
    shutdown_rx: &mut tokio::sync::oneshot::Receiver<()>,
) {
    // Spawn child process with stderr captured
    let cmd = Command::new(&config.command);
    let spawn_result = TokioChildProcess::builder(cmd.configure(|cmd| {
        cmd.args(&config.args);
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
    }))
    .stderr(std::process::Stdio::piped())
    .spawn();

    let (transport, stderr) = match spawn_result {
        Ok(result) => result,
        Err(e) => {
            let _ = tx.send(AppEvent::ConnectionFailed(idx, format!("Spawn failed: {e}")));
            return;
        }
    };

    // Initialize MCP connection
    let serve_result = tokio::time::timeout(
        std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
        ().serve(transport),
    )
    .await;

    let mut service = match serve_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = tx.send(AppEvent::ConnectionFailed(idx, format!("Initialize failed: {e}")));
            return;
        }
        Err(_) => {
            let _ = tx.send(AppEvent::ConnectionFailed(
                idx,
                format!("Initialize timed out ({CONNECT_TIMEOUT_SECS}s)"),
            ));
            return;
        }
    };

    let peer = service.peer().clone();
    let server_name = service.peer_info().map(|info| info.server_info.name.to_string());

    let _ = tx.send(AppEvent::ConnectionEstablished(idx, peer.clone(), server_name));

    // Spawn stderr reader task
    if let Some(stderr) = stderr {
        let tx_stderr = tx.clone();
        tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_stderr.send(AppEvent::StderrLine(idx, line)).is_err() {
                    break;
                }
            }
        });
    }

    spawn_refresh_capabilities(idx, &peer, tx);

    // Keep alive
    loop {
        tokio::select! {
            _ = &mut *shutdown_rx => {
                let _ = service.close().await;
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                if service.is_closed() {
                    let _ = tx.send(AppEvent::ConnectionLost(idx, "Server process exited".into()));
                    break;
                }
            }
        }
    }
}

async fn spawn_http_connection(
    idx: usize,
    config: &ServerConfig,
    tx: &mpsc::UnboundedSender<AppEvent>,
    shutdown_rx: &mut tokio::sync::oneshot::Receiver<()>,
) {
    let url = match &config.url {
        Some(u) => u.clone(),
        None => {
            let _ = tx.send(AppEvent::ConnectionFailed(
                idx,
                "HTTP transport requires 'url' field".into(),
            ));
            return;
        }
    };

    // Create HTTP transport
    let transport = rmcp::transport::StreamableHttpClientTransport::from_uri(url);

    let serve_result = tokio::time::timeout(
        std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
        rmcp::serve_client((), transport),
    )
    .await;

    let mut service = match serve_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let _ = tx.send(AppEvent::ConnectionFailed(
                idx,
                format!("HTTP initialize failed: {e}"),
            ));
            return;
        }
        Err(_) => {
            let _ = tx.send(AppEvent::ConnectionFailed(
                idx,
                format!("HTTP initialize timed out ({CONNECT_TIMEOUT_SECS}s)"),
            ));
            return;
        }
    };

    let peer = service.peer().clone();
    let server_name = service.peer_info().map(|info| info.server_info.name.to_string());

    let _ = tx.send(AppEvent::ConnectionEstablished(idx, peer.clone(), server_name));

    spawn_refresh_capabilities(idx, &peer, tx);

    // Keep alive (no stderr for HTTP)
    loop {
        tokio::select! {
            _ = &mut *shutdown_rx => {
                let _ = service.close().await;
                break;
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                if service.is_closed() {
                    let _ = tx.send(AppEvent::ConnectionLost(idx, "HTTP connection closed".into()));
                    break;
                }
            }
        }
    }
}

/// Refresh tools, resources, and prompts for a connected server.
pub fn spawn_refresh_capabilities(
    idx: usize,
    peer: &Peer<RoleClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let peer = peer.clone();
    let tx = tx.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        let (tools_result, resources_result, prompts_result) = tokio::join!(
            tokio::time::timeout(
                std::time::Duration::from_secs(CAPABILITY_TIMEOUT_SECS),
                peer.list_all_tools(),
            ),
            tokio::time::timeout(
                std::time::Duration::from_secs(CAPABILITY_TIMEOUT_SECS),
                peer.list_all_resources(),
            ),
            tokio::time::timeout(
                std::time::Duration::from_secs(CAPABILITY_TIMEOUT_SECS),
                peer.list_all_prompts(),
            ),
        );

        let elapsed = start.elapsed().as_millis() as u64;

        let tools = tools_result.ok().and_then(|r| r.ok()).unwrap_or_default();
        let resources = resources_result.ok().and_then(|r| r.ok()).unwrap_or_default();
        let prompts = prompts_result.ok().and_then(|r| r.ok()).unwrap_or_default();

        let _ = tx.send(AppEvent::CapabilitiesLoaded(idx, tools, resources, prompts, elapsed));
    });
}

/// Lightweight health check on an existing connection.
pub fn spawn_health_check(
    idx: usize,
    peer: &Peer<RoleClient>,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let peer = peer.clone();
    let tx = tx.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(CAPABILITY_TIMEOUT_SECS),
            peer.list_all_tools(),
        )
        .await;

        let elapsed = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(tools)) => {
                let _ = tx.send(AppEvent::HealthCheckResult(idx, Ok((tools, elapsed))));
            }
            Ok(Err(e)) => {
                let _ = tx.send(AppEvent::HealthCheckResult(
                    idx,
                    Err(format!("Health check failed: {e}")),
                ));
            }
            Err(_) => {
                let _ = tx.send(AppEvent::HealthCheckResult(
                    idx,
                    Err(format!("Health check timed out ({CAPABILITY_TIMEOUT_SECS}s)")),
                ));
            }
        }
    });
}
