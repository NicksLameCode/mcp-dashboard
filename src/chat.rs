use crate::chat_config::AiConfig;
use crate::connection::{ConnectionState, ManagedConnection};
use chrono::{DateTime, Local};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    Gemini,
    ClaudeCode,
    Cursor,
}

impl ProviderKind {
    pub fn label(&self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::Gemini => "Gemini",
            ProviderKind::ClaudeCode => "Claude Code",
            ProviderKind::Cursor => "Cursor",
        }
    }

    pub fn all() -> &'static [ProviderKind] {
        &[
            ProviderKind::Anthropic,
            ProviderKind::OpenAi,
            ProviderKind::Gemini,
            ProviderKind::ClaudeCode,
            ProviderKind::Cursor,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    #[allow(dead_code)]
    System,
    ToolCall,
    ToolResult,
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub tool_name: String,
    pub server_name: String,
    #[allow(dead_code)]
    pub is_result: bool,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    #[allow(dead_code)]
    pub timestamp: DateTime<Local>,
    pub tool_call: Option<ToolCallInfo>,
}

#[allow(dead_code)]
pub struct PendingToolCall {
    pub id: String,
    pub tool_name: String,
    pub server_index: usize,
    pub arguments: serde_json::Value,
}

pub struct ChatState {
    pub messages: Vec<ChatMessage>,
    pub input_mode: bool,
    pub input_buffer: String,
    pub scroll_offset: usize,
    pub is_streaming: bool,
    pub streaming_buffer: String,
    pub provider: ProviderKind,
    pub model: String,
    pub context_server_indices: Vec<usize>,
    pub context_cursor: usize, // for Tab-cycling through servers
    pub error: Option<String>,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub streaming_handle: Option<tokio::task::JoinHandle<()>>,
    pub pending_tool_calls: Vec<PendingToolCall>,
}

impl ChatState {
    pub fn new(ai_config: &AiConfig) -> Self {
        let provider = match ai_config.default_provider.as_str() {
            "openai" => ProviderKind::OpenAi,
            "gemini" => ProviderKind::Gemini,
            "claude_code" | "claude-code" => ProviderKind::ClaudeCode,
            "cursor" => ProviderKind::Cursor,
            _ => ProviderKind::Anthropic,
        };
        let model = match provider {
            ProviderKind::Anthropic => ai_config
                .anthropic
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
            ProviderKind::OpenAi => ai_config
                .openai
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "gpt-4o".into()),
            ProviderKind::Gemini => ai_config
                .gemini
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "gemini-2.0-flash".into()),
            ProviderKind::ClaudeCode => ai_config
                .claude_code
                .as_ref()
                .filter(|c| !c.model.is_empty())
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "claude-code".into()),
            ProviderKind::Cursor => ai_config
                .cursor
                .as_ref()
                .filter(|c| !c.model.is_empty())
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "auto".into()),
        };

        Self {
            messages: Vec::new(),
            input_mode: false,
            input_buffer: String::new(),
            scroll_offset: 0,
            is_streaming: false,
            streaming_buffer: String::new(),
            provider,
            model,
            context_server_indices: Vec::new(),
            context_cursor: 0,
            error: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            streaming_handle: None,
            pending_tool_calls: Vec::new(),
        }
    }

    pub fn cycle_provider(&mut self, ai_config: &AiConfig) {
        let providers = ProviderKind::all();
        let current_idx = providers
            .iter()
            .position(|p| *p == self.provider)
            .unwrap_or(0);
        let next = providers[(current_idx + 1) % providers.len()];
        self.provider = next;
        self.model = match next {
            ProviderKind::Anthropic => ai_config
                .anthropic
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
            ProviderKind::OpenAi => ai_config
                .openai
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "gpt-4o".into()),
            ProviderKind::Gemini => ai_config
                .gemini
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "gemini-2.0-flash".into()),
            ProviderKind::ClaudeCode => ai_config
                .claude_code
                .as_ref()
                .filter(|c| !c.model.is_empty())
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "claude-code".into()),
            ProviderKind::Cursor => ai_config
                .cursor
                .as_ref()
                .filter(|c| !c.model.is_empty())
                .map(|c| c.model.clone())
                .unwrap_or_else(|| "auto".into()),
        };
    }

    pub fn cycle_model(&mut self) {
        let models = match self.provider {
            ProviderKind::ClaudeCode => vec![
                "claude-code".into(),
                "sonnet".into(),
                "opus".into(),
                "haiku".into(),
            ],
            ProviderKind::Cursor => vec![
                "auto".into(),
                "claude-4.6-opus-high-thinking".into(),
                "claude-4.6-opus-high".into(),
                "claude-4.6-sonnet-medium-thinking".into(),
                "claude-4.6-sonnet-medium".into(),
                "gpt-5.4-medium".into(),
                "gpt-5.4-high".into(),
                "gemini-3.1-pro".into(),
            ],
            // API providers use a single configured model
            _ => return,
        };
        let current_idx = models.iter().position(|m: &String| *m == self.model).unwrap_or(0);
        self.model.clone_from(&models[(current_idx + 1) % models.len()]);
    }

    pub fn new_conversation(&mut self) {
        self.messages.clear();
        self.streaming_buffer.clear();
        self.error = None;
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
        self.pending_tool_calls.clear();
        self.scroll_offset = 0;
        self.is_streaming = false;
        if let Some(handle) = self.streaming_handle.take() {
            handle.abort();
        }
    }

    pub fn toggle_server_context(&mut self, idx: usize) {
        if let Some(pos) = self.context_server_indices.iter().position(|&i| i == idx) {
            self.context_server_indices.remove(pos);
        } else {
            self.context_server_indices.push(idx);
            self.context_server_indices.sort();
        }
    }

    pub fn trim_history(&mut self, max: usize) {
        if self.messages.len() > max {
            self.messages.drain(0..self.messages.len() - max);
        }
    }

    pub fn cancel_stream(&mut self) {
        if let Some(handle) = self.streaming_handle.take() {
            handle.abort();
        }
        self.is_streaming = false;
        self.streaming_buffer.clear();
        self.pending_tool_calls.clear();
    }
}

/// Build tool definitions from MCP server tools for API providers.
/// Returns (tool_name, server_index) mapping alongside the definitions.
pub fn build_tool_definitions(
    connections: &[ManagedConnection],
    indices: &[usize],
) -> (Vec<ToolDefinition>, Vec<(String, usize)>) {
    let mut defs = Vec::new();
    let mut tool_map = Vec::new(); // (tool_name, server_index)

    for &idx in indices {
        let conn = match connections.get(idx) {
            Some(c) => c,
            None => continue,
        };
        if !conn.is_connected() {
            continue;
        }
        for tool in &conn.tools {
            let name = tool.name.to_string();
            let description = tool.description.as_deref().unwrap_or("").to_string();
            let input_schema = serde_json::to_value(&*tool.input_schema).unwrap_or_default();

            // Prefix tool name with server index to disambiguate across servers
            let qualified_name = if indices.len() > 1 {
                format!("s{}_{}", idx, name)
            } else {
                name.clone()
            };

            defs.push(ToolDefinition {
                name: qualified_name.clone(),
                description,
                parameters: input_schema,
            });
            tool_map.push((qualified_name, idx));
        }
    }
    (defs, tool_map)
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Build a system prompt with full MCP server context for the selected servers.
pub fn build_system_prompt(connections: &[ManagedConnection], indices: &[usize]) -> String {
    let mut prompt = String::from(
        "You are an AI assistant helping a developer manage and understand their MCP \
         (Model Context Protocol) servers. Below is live information about the servers \
         they are currently monitoring. You can call their tools directly when asked.\n\n",
    );

    for &idx in indices {
        let conn = match connections.get(idx) {
            Some(c) => c,
            None => continue,
        };

        prompt.push_str(&format!("## Server: {}\n", conn.config.name));
        prompt.push_str(&format!("- Source: {}\n", conn.config.source.label()));
        prompt.push_str(&format!("- Transport: {:?}\n", conn.config.transport));
        prompt.push_str(&format!(
            "- Status: {}\n",
            match &conn.state {
                ConnectionState::Connected {
                    server_name, ..
                } => format!(
                    "Connected ({})",
                    server_name.as_deref().unwrap_or("unknown")
                ),
                ConnectionState::Connecting => "Connecting".into(),
                ConnectionState::Error(e) => format!("Error: {e}"),
                ConnectionState::Disconnected => "Disconnected".into(),
            }
        ));

        if !conn.tools.is_empty() {
            prompt.push_str(&format!("\n### Tools ({}):\n", conn.tools.len()));
            for tool in &conn.tools {
                prompt.push_str(&format!("- **{}**", tool.name));
                if let Some(desc) = &tool.description {
                    prompt.push_str(&format!(": {}", desc));
                }
                prompt.push('\n');
                if let Ok(schema) = serde_json::to_string(&*tool.input_schema) {
                    prompt.push_str(&format!("  Input schema: {}\n", schema));
                }
            }
        }

        if !conn.resources.is_empty() {
            prompt.push_str(&format!("\n### Resources ({}):\n", conn.resources.len()));
            for resource in &conn.resources {
                prompt.push_str(&format!("- **{}** ({})", resource.name, resource.uri));
                if let Some(desc) = &resource.description {
                    prompt.push_str(&format!(": {}", desc));
                }
                prompt.push('\n');
            }
        }

        if !conn.prompts.is_empty() {
            prompt.push_str(&format!("\n### Prompts ({}):\n", conn.prompts.len()));
            for p in &conn.prompts {
                prompt.push_str(&format!("- **{}**", p.name));
                if let Some(desc) = &p.description {
                    prompt.push_str(&format!(": {}", desc));
                }
                prompt.push('\n');
            }
        }

        let token_est = crate::tokens::estimate(&conn.tools, &conn.resources, &conn.prompts);
        prompt.push_str(&format!(
            "\nEstimated context cost: ~{} tokens\n\n---\n\n",
            token_est.display()
        ));
    }

    prompt
}
