use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub anthropic: Option<AnthropicConfig>,
    #[serde(default)]
    pub openai: Option<OpenAiConfig>,
    #[serde(default)]
    pub gemini: Option<GeminiConfig>,
    #[serde(default)]
    pub claude_code: Option<SubprocessConfig>,
    #[serde(default)]
    pub cursor: Option<SubprocessConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_anthropic_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default = "default_openai_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_gemini_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessConfig {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

fn default_provider() -> String {
    "anthropic".into()
}
fn default_anthropic_model() -> String {
    "claude-sonnet-4-20250514".into()
}
fn default_openai_model() -> String {
    "gpt-4o".into()
}
fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".into()
}
fn default_gemini_model() -> String {
    "gemini-2.0-flash".into()
}
fn default_max_tokens() -> usize {
    4096
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            anthropic: Some(AnthropicConfig {
                api_key: String::new(),
                model: default_anthropic_model(),
                max_tokens: default_max_tokens(),
            }),
            openai: Some(OpenAiConfig {
                api_key: String::new(),
                base_url: default_openai_base_url(),
                model: default_openai_model(),
                max_tokens: default_max_tokens(),
            }),
            gemini: Some(GeminiConfig {
                api_key: String::new(),
                model: default_gemini_model(),
                max_tokens: default_max_tokens(),
            }),
            claude_code: Some(SubprocessConfig {
                command: "claude".into(),
                args: vec!["--print".into()],
            }),
            cursor: Some(SubprocessConfig {
                command: "cursor".into(),
                args: vec!["--chat".into()],
            }),
        }
    }
}

fn ai_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/mcp-dashboard/ai.json")
}

pub fn load_ai_config() -> AiConfig {
    let path = ai_config_path();

    let mut config = if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => AiConfig::default(),
        }
    } else {
        // Create default config file
        let config = AiConfig::default();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = std::fs::write(&path, json);
        }
        config
    };

    // Environment variable fallbacks
    if let Some(ref mut anthropic) = config.anthropic {
        if anthropic.api_key.is_empty() {
            if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                anthropic.api_key = key;
            }
        }
    }
    if let Some(ref mut openai) = config.openai {
        if openai.api_key.is_empty() {
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                openai.api_key = key;
            }
        }
    }
    if let Some(ref mut gemini) = config.gemini {
        if gemini.api_key.is_empty() {
            if let Ok(key) = std::env::var("GEMINI_API_KEY") {
                gemini.api_key = key;
            }
        }
    }

    config
}
