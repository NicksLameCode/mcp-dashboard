use crate::app::AppEvent;
use chrono::{DateTime, Local};
use rmcp::model::CallToolRequestParams;
use rmcp::{Peer, RoleClient};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Default)]
pub struct InspectorState {
    pub selected_tool: usize,
    pub input_mode: bool,
    pub input_buffer: String,
    pub result_lines: Vec<String>,
    pub result_is_error: bool,
    pub result_scroll: usize,
    pub is_executing: bool,
}

#[derive(Debug, Clone)]
pub struct ProtocolEntry {
    pub timestamp: DateTime<Local>,
    pub server: String,
    pub direction: &'static str,
    pub method: String,
    pub summary: String,
    pub duration_ms: Option<u64>,
    pub is_error: bool,
}

pub fn spawn_execute_tool(
    server_idx: usize,
    peer: &Peer<RoleClient>,
    tool_name: &str,
    arguments_json: &str,
    tx: &mpsc::UnboundedSender<AppEvent>,
) {
    let peer = peer.clone();
    let tool_name = tool_name.to_string();
    let arguments_json = arguments_json.to_string();
    let tx = tx.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        // Parse arguments
        let arguments = if arguments_json.trim().is_empty() {
            None
        } else {
            match serde_json::from_str::<serde_json::Value>(&arguments_json) {
                Ok(serde_json::Value::Object(map)) => Some(map),
                Ok(_) => {
                    let _ = tx.send(AppEvent::ToolResult(
                        server_idx,
                        Err("Arguments must be a JSON object".into()),
                    ));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ToolResult(
                        server_idx,
                        Err(format!("Invalid JSON: {e}")),
                    ));
                    return;
                }
            }
        };

        let params = if let Some(args) = arguments {
            CallToolRequestParams::new(tool_name).with_arguments(args)
        } else {
            CallToolRequestParams::new(tool_name)
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            peer.call_tool(params),
        )
        .await;

        let elapsed = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(call_result)) => {
                let mut output = Vec::new();
                let is_error = call_result.is_error.unwrap_or(false);

                for content in &call_result.content {
                    if let Some(text) = content.raw.as_text() {
                        output.push(text.text.clone());
                    } else if content.raw.as_image().is_some() {
                        output.push("[image content]".to_string());
                    } else if content.raw.as_resource().is_some() {
                        output.push("[embedded resource]".to_string());
                    } else {
                        output.push("[unknown content type]".to_string());
                    }
                }

                if let Some(structured) = &call_result.structured_content {
                    if let Ok(pretty) = serde_json::to_string_pretty(structured) {
                        output.push(pretty);
                    }
                }

                let result_text = if output.is_empty() {
                    "(empty result)".to_string()
                } else {
                    output.join("\n")
                };

                let _ = tx.send(AppEvent::ToolResult(
                    server_idx,
                    Ok((result_text, elapsed, is_error)),
                ));
            }
            Ok(Err(e)) => {
                let _ = tx.send(AppEvent::ToolResult(
                    server_idx,
                    Err(format!("Tool call failed: {e}")),
                ));
            }
            Err(_) => {
                let _ = tx.send(AppEvent::ToolResult(
                    server_idx,
                    Err("Tool call timed out (30s)".to_string()),
                ));
            }
        }
    });
}
