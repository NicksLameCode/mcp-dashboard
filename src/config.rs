use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConfigSource {
    #[default]
    Manual,
    ClaudeCode,
    Cursor,
    ClaudeDesktop,
}

impl ConfigSource {
    pub fn label(&self) -> &'static str {
        match self {
            ConfigSource::Manual => "manual",
            ConfigSource::ClaudeCode => "claude-code",
            ConfigSource::Cursor => "cursor",
            ConfigSource::ClaudeDesktop => "claude-desktop",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[allow(dead_code)]
    #[serde(default = "default_type")]
    pub server_type: String,
    #[serde(default)]
    pub config_path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub transport: TransportType,
    #[serde(skip)]
    pub source: ConfigSource,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    #[default]
    Stdio,
    Http,
}

fn default_type() -> String {
    "unknown".into()
}

const SAMPLE_CONFIG: &str = r#"[
  {
    "name": "example-server",
    "command": "node",
    "args": ["dist/index.js"],
    "cwd": "/path/to/your/mcp-server",
    "env": {},
    "server_type": "node",
    "config_path": "/path/to/your/mcp-server/.mcp.json"
  }
]
"#;

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/mcp-dashboard/servers.json")
}

pub fn load_config() -> Result<Vec<ServerConfig>, String> {
    let mut configs = load_manual_config()?;
    let manual_keys: HashSet<String> = configs
        .iter()
        .map(config_key)
        .collect();

    // Auto-discover from other MCP clients
    let discovered = discover_all();
    for config in discovered {
        let key = config_key(&config);
        if !manual_keys.contains(&key) {
            configs.push(config);
        }
    }

    Ok(configs)
}

fn config_key(config: &ServerConfig) -> String {
    format!("{}:{}", config.command, config.args.join(","))
}

fn load_manual_config() -> Result<Vec<ServerConfig>, String> {
    let path = config_path();

    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, SAMPLE_CONFIG);
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    serde_json::from_str(&content).map_err(|e| format!("Invalid config: {e}"))
}

// --- Auto-discovery ---

/// Standard MCP config format used by Claude Desktop, Claude Code, Cursor
#[derive(Deserialize)]
struct ExternalMcpConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: HashMap<String, ExternalServerDef>,
}

#[derive(Deserialize)]
struct ExternalServerDef {
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

fn discover_all() -> Vec<ServerConfig> {
    let mut configs = Vec::new();
    configs.extend(discover_claude_code());
    configs.extend(discover_cursor());
    configs.extend(discover_claude_desktop());
    configs
}

fn discover_claude_code() -> Vec<ServerConfig> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    let paths = [
        home.join(".claude/.mcp.json"),
        home.join(".claude/mcp.json"),
    ];

    let mut configs = Vec::new();
    for path in &paths {
        configs.extend(parse_external_config(path, ConfigSource::ClaudeCode));
    }
    configs
}

fn discover_cursor() -> Vec<ServerConfig> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    let path = home.join(".cursor/mcp.json");
    parse_external_config(&path, ConfigSource::Cursor)
}

fn discover_claude_desktop() -> Vec<ServerConfig> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    let paths = [
        // Linux
        home.join(".config/claude/claude_desktop_config.json"),
        // macOS
        home.join("Library/Application Support/Claude/claude_desktop_config.json"),
    ];

    let mut configs = Vec::new();
    for path in &paths {
        configs.extend(parse_external_config(path, ConfigSource::ClaudeDesktop));
    }
    configs
}

fn parse_external_config(path: &PathBuf, source: ConfigSource) -> Vec<ServerConfig> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let external: ExternalMcpConfig = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    external
        .mcp_servers
        .into_iter()
        .filter_map(|(name, def)| {
            let command = def.command?;
            Some(ServerConfig {
                name,
                command,
                args: def.args,
                cwd: None,
                env: def.env,
                server_type: source.label().to_string(),
                config_path: Some(path.to_string_lossy().into_owned()),
                url: None,
                transport: TransportType::Stdio,
                source,
            })
        })
        .collect()
}
