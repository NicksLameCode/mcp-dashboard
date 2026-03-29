use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default = "default_type")]
    pub server_type: String,
    #[serde(default)]
    pub config_path: Option<String>,
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
    let path = config_path();

    if !path.exists() {
        // First run: create sample config
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
