use rmcp::model::{Prompt, Resource, Tool};

pub struct TokenEstimate {
    pub total: usize,
    pub tools: usize,
    pub resources: usize,
    pub prompts: usize,
}

impl TokenEstimate {
    pub fn severity_color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self.total {
            0..=1000 => Color::Green,
            1001..=5000 => Color::Yellow,
            5001..=10000 => Color::Rgb(255, 165, 0), // orange
            _ => Color::Red,
        }
    }

    pub fn display(&self) -> String {
        if self.total >= 1000 {
            format!("{:.1}k", self.total as f64 / 1000.0)
        } else {
            format!("{}", self.total)
        }
    }
}

/// Estimate token count for a server's MCP definitions.
/// Uses ~3.5 chars/token ratio for JSON, which matches cl100k_base averages.
pub fn estimate(tools: &[Tool], resources: &[Resource], prompts: &[Prompt]) -> TokenEstimate {
    let tools_tokens = estimate_tools(tools);
    let resources_tokens = estimate_resources(resources);
    let prompts_tokens = estimate_prompts(prompts);

    TokenEstimate {
        total: tools_tokens + resources_tokens + prompts_tokens,
        tools: tools_tokens,
        resources: resources_tokens,
        prompts: prompts_tokens,
    }
}

fn chars_to_tokens(chars: usize) -> usize {
    // ~3.5 characters per token for JSON content (conservative estimate)
    (chars as f64 / 3.5).ceil() as usize
}

fn estimate_tools(tools: &[Tool]) -> usize {
    let mut total_chars = 0;
    for tool in tools {
        total_chars += tool.name.len();
        if let Some(desc) = &tool.description {
            total_chars += desc.len();
        }
        // Input schema serialized as JSON
        if let Ok(schema_json) = serde_json::to_string(&*tool.input_schema) {
            total_chars += schema_json.len();
        }
        // Output schema if present
        if let Some(output) = &tool.output_schema {
            if let Ok(json) = serde_json::to_string(&**output) {
                total_chars += json.len();
            }
        }
    }
    chars_to_tokens(total_chars)
}

fn estimate_resources(resources: &[Resource]) -> usize {
    let mut total_chars = 0;
    for resource in resources {
        total_chars += resource.name.len();
        total_chars += resource.uri.len();
        if let Some(desc) = &resource.description {
            total_chars += desc.len();
        }
        if let Some(mime) = &resource.mime_type {
            total_chars += mime.len();
        }
    }
    chars_to_tokens(total_chars)
}

fn estimate_prompts(prompts: &[Prompt]) -> usize {
    let mut total_chars = 0;
    for prompt in prompts {
        total_chars += prompt.name.len();
        if let Some(desc) = &prompt.description {
            total_chars += desc.len();
        }
        if let Some(args) = &prompt.arguments {
            for arg in args {
                total_chars += arg.name.len();
                if let Some(desc) = &arg.description {
                    total_chars += desc.len();
                }
            }
        }
    }
    chars_to_tokens(total_chars)
}
