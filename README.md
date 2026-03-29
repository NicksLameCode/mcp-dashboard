# mcp-dashboard

A full-featured TUI dashboard for managing and monitoring [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) servers.

Maintains persistent connections to MCP servers, provides real-time health monitoring, tool/resource/prompt browsing, interactive tool execution, token cost estimation, and protocol-level visibility -- all from the terminal.

## Features

- **Persistent connections** with automatic health checks and reconnection
- **Tool execution** -- select a tool, enter JSON params, execute and see results
- **Resources & Prompts** -- browse the full MCP capability surface
- **Token cost estimation** -- see how many LLM context tokens each server's definitions consume
- **Auto-discovery** -- automatically finds servers from Claude Desktop, Claude Code, and Cursor configs
- **HTTP/SSE transport** -- connect to remote MCP servers via Streamable HTTP, not just stdio
- **Protocol log** -- see every MCP method call with timing
- **Server stderr capture** -- view server debug output in the Logs tab
- **Response time sparklines** -- visual performance trending
- **Search/filter** -- filter servers by name with `/`
- **Tab-based UI** -- Dashboard, Inspector, Protocol, and Logs tabs

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

On first run, a sample config is created at `~/.config/mcp-dashboard/servers.json`. Servers from Claude Desktop, Claude Code, and Cursor are auto-discovered.

### Keybindings

| Key | Action |
|-----|--------|
| `1` / `2` / `3` / `4` | Switch tabs (Dashboard / Inspector / Protocol / Logs) |
| `j` / `k` / `Up` / `Down` | Navigate server or tool list |
| `J` / `K` / `PgUp` / `PgDn` | Scroll detail panel |
| `Tab` | Cycle detail view: Tools / Resources / Prompts |
| `r` | Refresh all / reconnect failed servers |
| `c` | Connect or disconnect selected server |
| `e` | Edit selected server's config file |
| `/` | Search / filter servers by name |
| `?` | Help overlay |
| `q` / `Esc` | Quit |

**Inspector tab (Tab 2):**

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate tool list |
| `i` | Edit input parameters (JSON) |
| `Enter` | Execute selected tool |
| `Esc` | Exit input mode |

## Tabs

### Dashboard (Tab 1)

Server list with status, tool count, token estimate, and source badge. Detail panel shows server info, uptime, response time sparkline, and browsable tools/resources/prompts.

### Inspector (Tab 2)

Interactive tool execution. Select a tool, view its input schema, enter JSON arguments, and execute. Results are displayed inline.

### Protocol (Tab 3)

Log of MCP protocol operations (initialize, tools/list, tools/call) with direction arrows, timing, and error highlighting.

### Logs (Tab 4)

Per-server stderr output captured from child processes. Useful for debugging server-side issues.

## Configuration

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

### HTTP Server

```json
[
  {
    "name": "remote-server",
    "transport": "http",
    "url": "http://localhost:8080/mcp"
  }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Display name for the server |
| `command` | Yes (stdio) | Binary or command to run |
| `args` | No | Command line arguments |
| `cwd` | No | Working directory for the server process |
| `env` | No | Environment variables to set |
| `transport` | No | `stdio` (default) or `http` |
| `url` | Yes (http) | URL for HTTP transport |
| `server_type` | No | Label (not shown in v0.2.0+) |
| `config_path` | No | Path to server's config file (opened with `e`) |

### Auto-Discovery

mcp-dashboard automatically discovers servers from:

- **Claude Code**: `~/.claude/.mcp.json`, `~/.claude/mcp.json`
- **Cursor**: `~/.cursor/mcp.json`
- **Claude Desktop**: `~/.config/claude/claude_desktop_config.json`

Discovered servers appear alongside manually configured ones with a source badge.

## How It Works

On startup, mcp-dashboard establishes persistent connections to all configured servers:

1. Spawns each stdio server as a child process (or connects via HTTP)
2. Performs the MCP `initialize` handshake
3. Queries `tools/list`, `resources/list`, and `prompts/list` in parallel
4. Maintains the connection for health checks every 10 seconds
5. Captures stderr output for debugging
6. Detects server death and marks as error

Uses [rmcp](https://crates.io/crates/rmcp) for MCP protocol communication and [ratatui](https://crates.io/crates/ratatui) for the terminal UI.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
