use crate::config::ServerConfig;
use chrono::{DateTime, Local};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::ServiceExt;
use std::time::Instant;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ServerStatus {
    Checking,
    Healthy {
        tools: Vec<ToolInfo>,
        server_name: Option<String>,
        response_ms: u64,
    },
    Error(String),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ServerState {
    pub config: ServerConfig,
    pub status: ServerStatus,
    pub last_check: Option<DateTime<Local>>,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub server: String,
    pub message: String,
    pub is_error: bool,
}

pub async fn check_server(config: &ServerConfig) -> (ServerStatus, LogEntry) {
    let start = Instant::now();
    let name = config.name.clone();

    match check_server_inner(config).await {
        Ok((tools, server_name)) => {
            let elapsed = start.elapsed().as_millis() as u64;
            let tool_count = tools.len();
            let log = LogEntry {
                timestamp: Local::now(),
                server: name,
                message: format!(
                    "Healthy ({} tools, {}ms)",
                    tool_count, elapsed
                ),
                is_error: false,
            };
            (
                ServerStatus::Healthy {
                    tools,
                    server_name,
                    response_ms: elapsed,
                },
                log,
            )
        }
        Err(e) => {
            let log = LogEntry {
                timestamp: Local::now(),
                server: name,
                message: format!("Failed: {e}"),
                is_error: true,
            };
            (ServerStatus::Error(e), log)
        }
    }
}

async fn check_server_inner(
    config: &ServerConfig,
) -> Result<(Vec<ToolInfo>, Option<String>), String> {
    let mut cmd = Command::new(&config.command);

    let (transport, _stderr) = TokioChildProcess::builder(cmd.configure(|cmd| {
        cmd.args(&config.args);
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
    }))
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|e| format!("Spawn failed: {e}"))?;

    let client = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        ().serve(transport),
    )
    .await
    .map_err(|_| "Initialize timed out (15s)".to_string())?
    .map_err(|e| format!("Initialize failed: {e}"))?;

    let server_name = client
        .peer_info()
        .as_ref()
        .map(|info| info.server_info.name.to_string());

    let tools_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        client.list_all_tools(),
    )
    .await
    .map_err(|_| "tools/list timed out (10s)".to_string())?
    .map_err(|e| format!("tools/list failed: {e}"))?;

    let tools: Vec<ToolInfo> = tools_result
        .into_iter()
        .map(|t| ToolInfo {
            name: t.name.to_string(),
            description: t
                .description
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        })
        .collect();

    let _ = client.cancel().await;

    Ok((tools, server_name))
}
