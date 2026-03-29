# mcp-dashboard

A TUI dashboard for monitoring [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) servers.

Spawns each configured MCP server, performs a health check via the MCP protocol (initialize + tools/list), and displays real-time status, tool inventories, and logs in a terminal UI.

## Install

### From crates.io

```bash
cargo install mcp-dashboard
```

### From GitHub Releases

Download a pre-built binary from the [Releases](https://github.com/nickslamecode/mcp-dashboard/releases) page.

### Homebrew (macOS / Linux)

```bash
brew tap nickslamecode/mcp-dashboard
brew install mcp-dashboard
```

## Usage

```bash
mcp-dashboard
```

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` / `Up` / `Down` | Navigate server list |
| `r` | Refresh all servers |
| `e` | Edit selected server's config |
| `J` / `K` / `PageUp` / `PageDown` | Scroll tool list |
| `q` / `Esc` | Quit |

## Configuration

On first run, a sample config is created at:

```
~/.config/mcp-dashboard/servers.json
```

### Config Format

```json
[
  {
    "name": "my-server",
    "command": "/path/to/server-binary",
    "args": ["arg1", "arg2"],
    "cwd": "/path/to/working/directory",
    "env": {
      "API_KEY": "your-key"
    },
    "server_type": "rust",
    "config_path": "/path/to/project/.mcp.json"
  }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Display name for the server |
| `command` | Yes | Binary or command to run |
| `args` | No | Command line arguments |
| `cwd` | No | Working directory for the server process |
| `env` | No | Environment variables to set |
| `server_type` | No | Label shown in the UI (`rust`, `node`, etc.) |
| `config_path` | No | Path to the server's MCP config file (opened with `e` key) |

### Example: Node.js MCP Server

```json
[
  {
    "name": "my-mcp-server",
    "command": "node",
    "args": ["dist/index.js"],
    "cwd": "/home/user/my-mcp-server",
    "server_type": "node"
  }
]
```

### Example: Rust MCP Server

```json
[
  {
    "name": "my-rust-server",
    "command": "cargo",
    "args": ["run", "--release", "--bin", "my-mcp-server"],
    "cwd": "/home/user/my-rust-server",
    "server_type": "rust"
  }
]
```

## How It Works

Every 10 seconds (or on demand with `r`), the dashboard:

1. Spawns each MCP server as a child process via stdio transport
2. Sends an MCP `initialize` handshake
3. Calls `tools/list` to enumerate available tools
4. Records status, tool count, response time, and any errors
5. Gracefully shuts down the connection

Uses [rmcp](https://crates.io/crates/rmcp) for MCP protocol communication and [ratatui](https://crates.io/crates/ratatui) for the terminal UI.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
