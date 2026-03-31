use crate::app::AppEvent;
use crate::chat::{ChatMessage, ChatState, MessageRole, ProviderKind, ToolDefinition};
use crate::chat_config::AiConfig;
use crate::connection::ManagedConnection;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Spawn a chat request to the configured AI provider.
#[allow(clippy::too_many_arguments)]
pub fn spawn_chat_request(
    provider: ProviderKind,
    ai_config: &AiConfig,
    messages: &[ChatMessage],
    system_prompt: &str,
    connections: &[ManagedConnection],
    context_indices: &[usize],
    tx: mpsc::UnboundedSender<AppEvent>,
    chat: &mut ChatState,
) {
    // Abort any previous streaming handle to prevent orphaned tasks
    if let Some(handle) = chat.streaming_handle.take() {
        handle.abort();
    }

    // Build tool definitions for agentic mode
    let (tool_defs, tool_map) =
        crate::chat::build_tool_definitions(connections, context_indices);

    match provider {
        ProviderKind::Anthropic => {
            let config = match &ai_config.anthropic {
                Some(c) if !c.api_key.is_empty() => c.clone(),
                _ => {
                    let _ = tx.send(AppEvent::ChatError(
                        "No Anthropic API key. Set ANTHROPIC_API_KEY or edit ~/.config/mcp-dashboard/ai.json".into(),
                    ));
                    chat.is_streaming = false;
                    return;
                }
            };
            let msgs = convert_messages_anthropic(messages);
            let system = system_prompt.to_string();
            let tools = tool_defs.clone();
            let tmap = tool_map.clone();
            let handle = tokio::spawn(async move {
                anthropic_stream(config.api_key, config.model, config.max_tokens, system, msgs, tools, tmap, tx).await;
            });
            chat.streaming_handle = Some(handle);
        }
        ProviderKind::OpenAi => {
            let config = match &ai_config.openai {
                Some(c) if !c.api_key.is_empty() => c.clone(),
                _ => {
                    let _ = tx.send(AppEvent::ChatError(
                        "No OpenAI API key. Set OPENAI_API_KEY or edit ~/.config/mcp-dashboard/ai.json".into(),
                    ));
                    chat.is_streaming = false;
                    return;
                }
            };
            let msgs = convert_messages_openai(messages, system_prompt);
            let tools = tool_defs.clone();
            let tmap = tool_map.clone();
            let handle = tokio::spawn(async move {
                openai_stream(config.api_key, config.base_url, config.model, config.max_tokens, msgs, tools, tmap, tx).await;
            });
            chat.streaming_handle = Some(handle);
        }
        ProviderKind::Gemini => {
            let config = match &ai_config.gemini {
                Some(c) if !c.api_key.is_empty() => c.clone(),
                _ => {
                    let _ = tx.send(AppEvent::ChatError(
                        "No Gemini API key. Set GEMINI_API_KEY or edit ~/.config/mcp-dashboard/ai.json".into(),
                    ));
                    chat.is_streaming = false;
                    return;
                }
            };
            let msgs = convert_messages_gemini(messages);
            let system = system_prompt.to_string();
            let handle = tokio::spawn(async move {
                gemini_stream(config.api_key, config.model, config.max_tokens, system, msgs, tx).await;
            });
            chat.streaming_handle = Some(handle);
        }
        ProviderKind::ClaudeCode => {
            let config = match &ai_config.claude_code {
                Some(c) if !c.command.is_empty() => c.clone(),
                _ => {
                    let _ = tx.send(AppEvent::ChatError(
                        "Claude Code not configured. Ensure 'claude' is installed.".into(),
                    ));
                    chat.is_streaming = false;
                    return;
                }
            };
            let prompt = build_subprocess_prompt(messages, system_prompt, &tool_defs);
            let model = chat.model.clone();
            let tmap = tool_map.clone();
            let handle = tokio::spawn(async move {
                subprocess_chat(config.command, config.args, config.api_key, model, prompt, tmap, tx).await;
            });
            chat.streaming_handle = Some(handle);
        }
        ProviderKind::Cursor => {
            let config = match &ai_config.cursor {
                Some(c) if !c.command.is_empty() => c.clone(),
                _ => {
                    let _ = tx.send(AppEvent::ChatError(
                        "Cursor not configured. Ensure 'cursor-agent' is installed.".into(),
                    ));
                    chat.is_streaming = false;
                    return;
                }
            };
            let prompt = build_subprocess_prompt(messages, system_prompt, &tool_defs);
            let model = chat.model.clone();
            let tmap = tool_map.clone();
            let handle = tokio::spawn(async move {
                subprocess_chat(config.command, config.args, config.api_key, model, prompt, tmap, tx).await;
            });
            chat.streaming_handle = Some(handle);
        }
    }
}

fn tools_to_anthropic(defs: &[ToolDefinition]) -> Vec<serde_json::Value> {
    defs.iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "description": d.description,
                "input_schema": d.parameters,
            })
        })
        .collect()
}

fn tools_to_openai(defs: &[ToolDefinition]) -> Vec<serde_json::Value> {
    defs.iter()
        .map(|d| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": d.name,
                    "description": d.description,
                    "parameters": d.parameters,
                }
            })
        })
        .collect()
}

/// Resolve a tool call name back to the server index and original tool name.
fn resolve_tool(name: &str, tool_map: &[(String, usize)]) -> Option<(usize, String)> {
    for (qualified, server_idx) in tool_map {
        if qualified == name {
            // Strip server prefix if present (s0_, s1_, etc.)
            let original_name = if name.starts_with('s') && name.contains('_') {
                name.split_once('_').map(|x| x.1).unwrap_or(name)
            } else {
                name
            };
            return Some((*server_idx, original_name.to_string()));
        }
    }
    None
}

// ── Anthropic Messages API ──────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: usize,
    system: String,
    messages: Vec<AnthropicMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Serialize, Clone)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value, // String or array of content blocks
}

#[derive(Deserialize)]
struct AnthropicSseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<AnthropicDelta>,
    #[serde(default)]
    message: Option<AnthropicMessageInfo>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    content_block: Option<AnthropicContentBlock>,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    #[serde(default, rename = "type")]
    _delta_type: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicMessageInfo {
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: usize,
    #[serde(default)]
    output_tokens: usize,
}

fn convert_messages_anthropic(messages: &[ChatMessage]) -> Vec<AnthropicMessage> {
    let mut result = Vec::new();
    for m in messages {
        match m.role {
            MessageRole::User => {
                result.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: serde_json::Value::String(m.content.clone()),
                });
            }
            MessageRole::Assistant => {
                result.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::Value::String(m.content.clone()),
                });
            }
            MessageRole::ToolCall => {
                // Tool use from assistant — already in the assistant message via streaming
            }
            MessageRole::ToolResult => {
                // Send tool results as user messages with tool_result content blocks
                if let Some(ref info) = m.tool_call {
                    result.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: serde_json::json!([{
                            "type": "tool_result",
                            "tool_use_id": info.tool_name.clone(), // we store tool_use_id in tool_name for Anthropic
                            "content": m.content.clone(),
                        }]),
                    });
                }
            }
            MessageRole::System => {}
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_stream(
    api_key: String,
    model: String,
    max_tokens: usize,
    system: String,
    messages: Vec<AnthropicMessage>,
    tools: Vec<ToolDefinition>,
    tool_map: Vec<(String, usize)>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let client = reqwest::Client::new();

    let body = AnthropicRequest {
        model,
        max_tokens,
        system,
        messages,
        stream: true,
        tools: tools_to_anthropic(&tools),
    };

    let response = match client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AppEvent::ChatError(format!("Network error: {e}")));
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = tx.send(AppEvent::ChatError(format!(
            "Anthropic API error {status}: {body}"
        )));
        return;
    }

    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;

    // Track current content block for tool_use detection
    let mut current_block_type: Option<String> = None;
    let mut current_tool_use_id = String::new();
    let mut current_tool_name = String::new();
    let mut current_tool_input = String::new();

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::ChatError(format!("Stream error: {e}")));
                return;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete SSE events (separated by double newlines)
        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(event) = serde_json::from_str::<AnthropicSseEvent>(data) {
                        match event.event_type.as_str() {
                            "message_start" => {
                                if let Some(msg) = &event.message {
                                    if let Some(usage) = &msg.usage {
                                        input_tokens = usage.input_tokens;
                                    }
                                }
                            }
                            "content_block_start" => {
                                if let Some(cb) = &event.content_block {
                                    current_block_type = Some(cb.block_type.clone());
                                    if cb.block_type == "tool_use" {
                                        current_tool_use_id =
                                            cb.id.clone().unwrap_or_default();
                                        current_tool_name =
                                            cb.name.clone().unwrap_or_default();
                                        current_tool_input.clear();
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = &event.delta {
                                    match current_block_type.as_deref() {
                                        Some("text") => {
                                            if let Some(text) = &delta.text {
                                                let _ = tx.send(AppEvent::ChatToken(
                                                    text.clone(),
                                                ));
                                            }
                                        }
                                        Some("tool_use") => {
                                            if let Some(json) = &delta.partial_json {
                                                current_tool_input.push_str(json);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "content_block_stop" => {
                                if current_block_type.as_deref() == Some("tool_use") {
                                    // Parse tool input and emit tool call event
                                    let args: serde_json::Value =
                                        serde_json::from_str(&current_tool_input)
                                            .unwrap_or(serde_json::Value::Object(
                                                Default::default(),
                                            ));

                                    if let Some((server_idx, _original_name)) =
                                        resolve_tool(&current_tool_name, &tool_map)
                                    {
                                        let _ = tx.send(AppEvent::ChatToolCall {
                                            id: current_tool_use_id.clone(),
                                            name: current_tool_name.clone(),
                                            server_idx,
                                            args,
                                        });
                                    }
                                }
                                current_block_type = None;
                            }
                            "message_delta" => {
                                if let Some(usage) = &event.usage {
                                    output_tokens = usage.output_tokens;
                                }
                            }
                            "message_stop" => {}
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // Process any remaining data in buffer (stream ended without trailing \n\n)
    if !buffer.trim().is_empty() {
        for line in buffer.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<AnthropicSseEvent>(data) {
                    if let Some(delta) = &event.delta {
                        if let Some(text) = &delta.text {
                            let _ = tx.send(AppEvent::ChatToken(text.clone()));
                        }
                    }
                    if let Some(usage) = &event.usage {
                        output_tokens = usage.output_tokens;
                    }
                }
            }
        }
    }

    let _ = tx.send(AppEvent::ChatResponseComplete {
        input_tokens,
        output_tokens,
    });
}

// ── OpenAI-compatible Chat Completions API ──────────────────────────────

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<serde_json::Value>,
}

#[derive(Serialize, Clone)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiSseChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    delta: Option<OpenAiDelta>,
}

#[derive(Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Deserialize)]
struct OpenAiToolCallDelta {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Deserialize)]
struct OpenAiFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: usize,
    #[serde(default)]
    completion_tokens: usize,
}

fn convert_messages_openai(messages: &[ChatMessage], system_prompt: &str) -> Vec<OpenAiMessage> {
    let mut result = vec![OpenAiMessage {
        role: "system".to_string(),
        content: system_prompt.to_string(),
    }];

    for m in messages {
        let role = match m.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            // Include tool calls/results as assistant/tool messages for agentic continuity
            MessageRole::ToolCall => "assistant",
            MessageRole::ToolResult => "tool",
            MessageRole::System => "system",
        };
        result.push(OpenAiMessage {
            role: role.to_string(),
            content: m.content.clone(),
        });
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn openai_stream(
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: usize,
    messages: Vec<OpenAiMessage>,
    tools: Vec<ToolDefinition>,
    tool_map: Vec<(String, usize)>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let body = OpenAiRequest {
        model,
        max_tokens,
        messages,
        stream: true,
        tools: tools_to_openai(&tools),
    };

    let response = match client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AppEvent::ChatError(format!("Network error: {e}")));
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = tx.send(AppEvent::ChatError(format!(
            "OpenAI API error {status}: {body}"
        )));
        return;
    }

    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    // Track accumulating tool call
    let mut tool_call_id = String::new();
    let mut tool_call_name = String::new();
    let mut tool_call_args = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::ChatError(format!("Stream error: {e}")));
                return;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        // Emit any pending tool call
                        if !tool_call_name.is_empty() {
                            let args: serde_json::Value =
                                serde_json::from_str(&tool_call_args)
                                    .unwrap_or(serde_json::Value::Object(Default::default()));
                            if let Some((server_idx, _)) =
                                resolve_tool(&tool_call_name, &tool_map)
                            {
                                let _ = tx.send(AppEvent::ChatToolCall {
                                    id: tool_call_id.clone(),
                                    name: tool_call_name.clone(),
                                    server_idx,
                                    args,
                                });
                            }
                            tool_call_name.clear();
                            tool_call_args.clear();
                        }
                        continue;
                    }
                    if let Ok(chunk) = serde_json::from_str::<OpenAiSseChunk>(data) {
                        for choice in &chunk.choices {
                            if let Some(delta) = &choice.delta {
                                if let Some(content) = &delta.content {
                                    let _ = tx.send(AppEvent::ChatToken(content.clone()));
                                }
                                // Tool call streaming
                                if let Some(tool_calls) = &delta.tool_calls {
                                    for tc in tool_calls {
                                        if let Some(id) = &tc.id {
                                            tool_call_id = id.clone();
                                        }
                                        if let Some(func) = &tc.function {
                                            if let Some(name) = &func.name {
                                                tool_call_name = name.clone();
                                            }
                                            if let Some(args) = &func.arguments {
                                                tool_call_args.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(usage) = &chunk.usage {
                            input_tokens = usage.prompt_tokens;
                            output_tokens = usage.completion_tokens;
                        }
                    }
                }
            }
        }
    }

    // Process any remaining buffer (stream ended without trailing \n\n)
    if !buffer.trim().is_empty() {
        for line in buffer.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }
                if let Ok(chunk) = serde_json::from_str::<OpenAiSseChunk>(data) {
                    for choice in &chunk.choices {
                        if let Some(delta) = &choice.delta {
                            if let Some(content) = &delta.content {
                                let _ = tx.send(AppEvent::ChatToken(content.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    // Emit any pending tool call at end of stream
    if !tool_call_name.is_empty() {
        let args: serde_json::Value = serde_json::from_str(&tool_call_args)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        if let Some((server_idx, _)) = resolve_tool(&tool_call_name, &tool_map) {
            let _ = tx.send(AppEvent::ChatToolCall {
                id: tool_call_id.clone(),
                name: tool_call_name.clone(),
                server_idx,
                args,
            });
        }
    }

    let _ = tx.send(AppEvent::ChatResponseComplete {
        input_tokens,
        output_tokens,
    });
}

// ── Google Gemini API ───────────────────────────────────────────────────

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Serialize, Clone)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Clone)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: usize,
}

#[derive(Deserialize)]
struct GeminiStreamResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiContentResponse>,
}

#[derive(Deserialize)]
struct GeminiContentResponse {
    #[serde(default)]
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
struct GeminiPartResponse {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: usize,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: usize,
}

fn convert_messages_gemini(messages: &[ChatMessage]) -> Vec<GeminiContent> {
    messages
        .iter()
        .filter_map(|m| {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "model",
                MessageRole::ToolCall | MessageRole::ToolResult | MessageRole::System => {
                    return None
                }
            };
            Some(GeminiContent {
                role: role.to_string(),
                parts: vec![GeminiPart {
                    text: m.content.clone(),
                }],
            })
        })
        .collect()
}

async fn gemini_stream(
    api_key: String,
    model: String,
    max_tokens: usize,
    system: String,
    contents: Vec<GeminiContent>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    let client = reqwest::Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse&key={api_key}"
    );

    let body = GeminiRequest {
        contents,
        system_instruction: Some(GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart { text: system }],
        }),
        generation_config: Some(GeminiGenerationConfig {
            max_output_tokens: max_tokens,
        }),
    };

    let response = match client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AppEvent::ChatError(format!("Network error: {e}")));
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = tx.send(AppEvent::ChatError(format!(
            "Gemini API error {status}: {body}"
        )));
        return;
    }

    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::ChatError(format!("Stream error: {e}")));
                return;
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in event_text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(resp) = serde_json::from_str::<GeminiStreamResponse>(data) {
                        for candidate in &resp.candidates {
                            if let Some(content) = &candidate.content {
                                for part in &content.parts {
                                    if let Some(text) = &part.text {
                                        let _ = tx.send(AppEvent::ChatToken(text.clone()));
                                    }
                                }
                            }
                        }
                        if let Some(usage) = &resp.usage_metadata {
                            input_tokens = usage.prompt_token_count;
                            output_tokens = usage.candidates_token_count;
                        }
                    }
                }
            }
        }
    }

    // Process any remaining buffer
    if !buffer.trim().is_empty() {
        for line in buffer.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(resp) = serde_json::from_str::<GeminiStreamResponse>(data) {
                    for candidate in &resp.candidates {
                        if let Some(content) = &candidate.content {
                            for part in &content.parts {
                                if let Some(text) = &part.text {
                                    let _ = tx.send(AppEvent::ChatToken(text.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = tx.send(AppEvent::ChatResponseComplete {
        input_tokens,
        output_tokens,
    });
}

// ── Subprocess providers (Claude Code, Cursor) ─────────────────────────

fn build_subprocess_prompt(
    messages: &[ChatMessage],
    system_prompt: &str,
    tool_defs: &[crate::chat::ToolDefinition],
) -> String {
    let mut prompt = format!("Context:\n{system_prompt}\n\n");

    if !tool_defs.is_empty() {
        prompt.push_str("## TOOLS — YOU MUST USE THESE\n\n");
        prompt.push_str(
            "You have DIRECT access to the following tools. \
             You MUST call them when the user asks for any data they can provide. \
             Do NOT say you cannot access them — you CAN and MUST use them.\n\n\
             To call a tool, include this EXACT format in your response:\n\n\
             ```tool_call\n\
             {\"tool\": \"tool_name\", \"arguments\": {\"arg1\": \"value1\"}}\n\
             ```\n\n\
             The tool will be executed and the result will be provided back to you \
             in a follow-up message. You may call multiple tools.\n\n\
             Available tools:\n",
        );
        for tool in tool_defs {
            prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
            if let Some(props) = tool.parameters.get("properties") {
                prompt.push_str(&format!("  Parameters: {}\n", props));
            }
        }
        prompt.push('\n');
    }

    prompt.push_str("Conversation:\n");
    for m in messages {
        let role = match m.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::ToolCall => "Tool Call",
            MessageRole::ToolResult => "Tool Result",
            MessageRole::System => "System",
        };
        prompt.push_str(&format!("{role}: {}\n", m.content));
    }
    prompt
}

async fn subprocess_chat(
    command: String,
    args: Vec<String>,
    api_key: String,
    model: String,
    prompt: String,
    tool_map: Vec<(String, usize)>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    use tokio::io::AsyncReadExt;

    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&args);

    // Pass API key via environment variable if configured
    if !api_key.is_empty() && command.contains("cursor-agent") {
        cmd.env("CURSOR_API_KEY", &api_key);
    }

    // Pass --model flag if a non-default model is specified
    // Both claude and cursor-agent support --model
    let is_default_model = model.is_empty() || model == "claude-code" || model == "auto";
    if !is_default_model {
        cmd.arg("--model").arg(&model);
    }

    // Both claude and cursor-agent accept prompt via -p flag
    let uses_prompt_flag = command.contains("claude") || command.contains("cursor-agent");
    if uses_prompt_flag {
        cmd.arg("-p").arg(&prompt);
    } else {
        // For other tools, try stdin
        cmd.stdin(std::process::Stdio::piped());
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(AppEvent::ChatError(format!(
                "Failed to spawn {command}: {e}. Is it installed?"
            )));
            return;
        }
    };

    // If not using -p flag, write prompt to stdin
    if !uses_prompt_flag {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(prompt.as_bytes()).await;
            drop(stdin);
        }
    }

    // Read stderr in a separate task so we can show real error messages
    let stderr_handle = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).to_string()
        })
    });

    // Buffer all stdout, then try to parse as JSON to extract the result text.
    let mut full_output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    full_output.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ChatError(format!("Read error: {e}")));
                    return;
                }
            }
        }
    }

    let status = child.wait().await;
    let exit_ok = status.map(|s| s.success()).unwrap_or(false);

    if !exit_ok {
        // Show the actual stderr message instead of a generic error
        let stderr_msg = if let Some(handle) = stderr_handle {
            handle.await.unwrap_or_default().trim().to_string()
        } else {
            String::new()
        };
        let err = if stderr_msg.is_empty() {
            format!("{command} exited with error")
        } else {
            stderr_msg
        };
        let _ = tx.send(AppEvent::ChatError(err));
        return;
    }

    // Try to parse as JSON and extract "result" field + token usage
    let mut input_tokens: usize = 0;
    let mut output_tokens: usize = 0;
    let response_text;
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&full_output) {
        response_text = json["result"].as_str().unwrap_or(&full_output).to_string();
        if let Some(usage) = json.get("usage") {
            input_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as usize
                + usage["cache_read_input_tokens"].as_u64().unwrap_or(0) as usize
                + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0) as usize;
            output_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as usize;
        }
    } else {
        response_text = full_output;
    }

    // Parse tool calls from the response text (```tool_call\n{...}\n```)
    let mut has_tool_calls = false;
    let mut remaining_text = String::new();
    let mut rest = response_text.as_str();
    while let Some(start) = rest.find("```tool_call") {
        // Add text before the tool call block
        remaining_text.push_str(&rest[..start]);
        rest = &rest[start..];

        // Find the JSON content between ```tool_call and ```
        let json_start = rest.find('\n').map(|i| i + 1);
        let json_end = rest[3..].find("```").map(|i| i + 3);

        if let (Some(js), Some(je)) = (json_start, json_end) {
            let json_str = rest[js..je].trim();
            if let Ok(call) = serde_json::from_str::<serde_json::Value>(json_str) {
                let tool_name = call["tool"].as_str().unwrap_or("").to_string();
                let arguments = call.get("arguments").cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));

                if let Some((server_idx, _)) = resolve_tool(&tool_name, &tool_map) {
                    has_tool_calls = true;
                    let id = format!("sub_{:x}", std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos());
                    let _ = tx.send(AppEvent::ChatToolCall {
                        id,
                        name: tool_name,
                        server_idx,
                        args: arguments,
                    });
                }
            }
            // Skip past closing ```
            let end_pos = je + 3;
            rest = if end_pos < rest.len() { &rest[end_pos..] } else { "" };
        } else {
            // Malformed block — include as text
            remaining_text.push_str(rest);
            rest = "";
        }
    }
    remaining_text.push_str(rest);

    // Send the non-tool-call text as a chat token
    let text_to_send = remaining_text.trim().to_string();
    if !text_to_send.is_empty() {
        let _ = tx.send(AppEvent::ChatToken(text_to_send));
    }

    if !has_tool_calls {
        let _ = tx.send(AppEvent::ChatResponseComplete {
            input_tokens,
            output_tokens,
        });
    }
    // If tool calls were emitted, the agentic loop in app.rs will handle
    // re-invocation after tool results come back.
}
