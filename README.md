<div align="center">

# mcp-dashboard

**The terminal dashboard for MCP servers. Now with AI Chat.**

Manage, monitor, and debug your [Model Context Protocol](https://modelcontextprotocol.io/) servers from a single pane of glass -- without leaving the terminal. **Talk to AI about your servers and let it call tools on your behalf.**

[![Crates.io](https://img.shields.io/crates/v/mcp-dashboard.svg?style=flat-square)](https://crates.io/crates/mcp-dashboard)
[![Downloads](https://img.shields.io/crates/d/mcp-dashboard.svg?style=flat-square)](https://crates.io/crates/mcp-dashboard)
[![License](https://img.shields.io/crates/l/mcp-dashboard.svg?style=flat-square)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange?style=flat-square&logo=rust)](https://www.rust-lang.org)

[Features](#features) &bull; [Performance](#performance) &bull; [Install](#install) &bull; [Quick Start](#quick-start) &bull; [Tabs](#tabs) &bull; [AI Chat](#5--chat-new) &bull; [Configuration](#configuration) &bull; [Keybindings](#keybindings)

</div>

<br>

<p align="center">
  <img src="assets/demo.gif" alt="mcp-dashboard demo" width="100%">
</p>

## Why mcp-dashboard?

Most MCP tooling is browser-based, Python-heavy, or requires Docker. **mcp-dashboard** is a single Rust binary that installs in seconds and runs anywhere -- your laptop, a server over SSH, a CI runner.

- **Zero dependencies** -- no Node, no Python, no Docker
- **Auto-discovers** servers from Claude Desktop, Claude Code, and Cursor
- **Persistent connections** -- no more spawning/killing processes every health check
- **Token cost estimation** -- see how much context window each server eats (unique to this tool)
- **Tool execution** -- call any MCP tool directly from the terminal
- **AI Chat** -- talk to Claude, GPT, Gemini, or Claude Code about your servers, with full agentic tool execution

## Features

| Feature | Description |
|---------|-------------|
| **Dashboard** | Real-time server health, tool counts, response time sparklines, token estimates |
| **Inspector** | Browse tools, enter JSON params, execute and see results inline |
| **AI Chat** | Converse with AI about your MCP servers -- 5 providers, streaming, agentic tool execution |
| **Protocol Log** | Every MCP method call with direction, timing, and error highlighting |
| **Server Logs** | Live stderr capture from server processes for debugging |
| **Auto-Discovery** | Finds servers from `~/.claude/.mcp.json`, `~/.cursor/mcp.json`, Claude Desktop config |
| **HTTP Transport** | Connect to remote MCP servers via Streamable HTTP, not just local stdio |
| **Multi-Provider AI** | Anthropic Claude, OpenAI/GPT, Google Gemini, Claude Code CLI, Cursor CLI |
| **Agentic Tools** | AI calls MCP tools directly during chat, results inline + logged to Protocol tab |
| **Search/Filter** | Filter servers by name with `/` -- handles large server collections |
| **Token Estimation** | Color-coded context window cost per server (green/yellow/orange/red) |
| **Sparklines** | Mini response time graphs showing performance trends |
| **Help Overlay** | Press `?` for a complete keybinding reference |

## Performance

Every MCP tool in your IDE (Claude Code, Cursor, Claude Desktop) pays a hidden tax: **spawn a process, initialize the MCP handshake, make your call, kill the process.** Every single time. mcp-dashboard eliminates this by holding persistent connections -- connect once, call forever.

We benchmarked real production MCP servers to quantify the difference.

### The Numbers

Tested on real servers with the included `mcp-latency` benchmark tool. Cold start = traditional (spawn + init + call + shutdown per interaction). Warm call = dashboard persistent connection.

#### Rust MCP Server (`db-tunnel`, 9 tools)

| Metric | Cold Start (traditional) | Warm Call (dashboard) | Speedup |
|--------|-------------------------:|----------------------:|--------:|
| **Mean** | 129.9ms | **0.1ms** | **1,846x** |
| p50 | 128.2ms | 0.1ms | 1,282x |
| p95 | 140.2ms | 0.1ms | 1,402x |

Where the time goes (cold start): Spawn + Init = **128.6ms** (99%), actual tool call = 0.4ms (1%).

#### Node.js MCP Server (`mobile-api`, 10 tools)

| Metric | Cold Start (traditional) | Warm Call (dashboard) | Speedup |
|--------|-------------------------:|----------------------:|--------:|
| **Mean** | 123.2ms | **0.5ms** | **272x** |
| p50 | 124.5ms | 0.4ms | 311x |
| p95 | 129.5ms | 0.6ms | 216x |

Where the time goes (cold start): Spawn + Init = **115.4ms** (94%), actual tool call = 2.8ms (6%).

### Throughput

Persistent connections don't just reduce latency -- they unlock burst throughput that's impossible with cold starts:

| Scenario | Rust Server | Node.js Server |
|----------|------------:|--------------:|
| Burst (10 rapid calls) | **0.8ms total** (12,629 calls/sec) | **6.0ms total** (1,673 calls/sec) |
| 10 calls traditional | 1,299ms (1.3s) | 1,232ms (1.2s) |
| **Speedup for 10 calls** | **10x** | **10x** |

### What This Means in Practice

Every time an AI agent calls an MCP tool through the traditional path, it waits **~130ms** of pure overhead before the tool even starts executing. In an agentic session with 20 tool calls, that's **2.6 seconds of wasted time** just on spawn/init cycles.

With mcp-dashboard's persistent connections:
- **100 tool calls** save **13 seconds** of overhead (Rust server) or **12 seconds** (Node.js server)
- An **agentic AI session** with 20 tool calls runs in **2ms** of connection overhead instead of **2,600ms**
- **Health checks** every 10 seconds cost **0.1ms** instead of spawning a new process

The bottleneck was never the tool -- it was the connection lifecycle. mcp-dashboard removes it entirely.

### Run the Benchmark Yourself

The benchmark tool is included:

```bash
cargo run --release --bin mcp-latency -- /path/to/your/mcp-server [args...]

# Examples:
cargo run --release --bin mcp-latency -- node dist/index.js
cargo run --release --bin mcp-latency -- /path/to/rust-mcp-server --flag
```

It runs 20 cold-start rounds and 50 warm-call rounds, then shows a comparison with percentiles and throughput.

## Install

### Cargo (recommended)

```bash
cargo install mcp-dashboard
```

### Homebrew

```bash
brew tap nickslamecode/mcp-dashboard
brew install mcp-dashboard
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/NicksLameCode/mcp-dashboard/releases/latest).

### From Source

```bash
git clone https://github.com/NicksLameCode/mcp-dashboard.git
cd mcp-dashboard
cargo install --path .
```

## Quick Start

```bash
# Just run it -- servers are auto-discovered from Claude/Cursor configs
mcp-dashboard
```

That's it. If you have MCP servers configured in Claude Desktop, Claude Code, or Cursor, they'll appear automatically.

To add servers manually, edit `~/.config/mcp-dashboard/servers.json` (created on first run):

```json
[
  {
    "name": "my-server",
    "command": "node",
    "args": ["dist/index.js"],
    "cwd": "/path/to/server"
  }
]
```

## Tabs

### 1 &mdash; Dashboard

The main view. Server list with status indicators, tool/resource/prompt counts, token cost estimates, and source badges. The detail panel shows:

- Server name, uptime, and response time
- Response time sparkline (last 60 checks)
- Token cost breakdown (tools/resources/prompts)
- Browsable tool, resource, and prompt lists (cycle with `Tab`)

### 2 &mdash; Inspector

Interactive tool execution. Select a tool from the left panel, view its input schema, type JSON arguments, and press `Enter` to execute. Results appear inline with error highlighting.

### 3 &mdash; Protocol

A chronological log of every MCP protocol operation -- `initialize`, `tools/list`, `tools/call`, etc. Shows direction (`→` sent, `←` received), server name, method, result summary, and round-trip time.

### 4 &mdash; Logs

Per-server stderr output captured from child processes. Select a server to view its debug output. Useful for diagnosing startup failures, malformed responses, or server-side errors.

### 5 &mdash; Chat (NEW)

A full AI conversation interface built into the dashboard. Talk to AI about your MCP servers and let it call tools on your behalf.

**5 AI Providers:**

| Provider | How it connects | Tool execution |
|----------|----------------|----------------|
| **Anthropic Claude** | Streaming API with `x-api-key` | Native `tool_use` blocks |
| **OpenAI / GPT** | Streaming API (also works with Ollama, LM Studio, Azure) | Function calling |
| **Google Gemini** | Streaming API with `key` param | Coming soon |
| **Claude Code** | Subprocess via `claude --print` | Via prompt context |
| **Cursor** | Subprocess via CLI | Via prompt context |

**Multi-Server Context:** Select which servers the AI knows about. Press `Tab` to cycle, `Space` to toggle. The AI sees full tool schemas, resource URIs, prompt definitions, and connection status for selected servers.

**Agentic Tool Execution:** The AI can call MCP tools directly during conversation. When it does:
- Tool call + arguments appear inline in the chat
- The tool executes against the real MCP server (30s timeout)
- Results appear inline and are logged to the Protocol tab
- The AI continues the conversation with the tool results

**Streaming:** Responses stream in real-time with a block cursor. Press `Esc` to cancel mid-stream.

**Quick start:**

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."

# Launch the dashboard, press 5 for Chat
mcp-dashboard
```

Or configure all providers in `~/.config/mcp-dashboard/ai.json` (auto-created on first run).

## Configuration

### Stdio Server (default)

```json
[
  {
    "name": "my-server",
    "command": "/path/to/binary",
    "args": ["--flag", "value"],
    "cwd": "/working/directory",
    "env": { "API_KEY": "sk-..." },
    "config_path": "/path/to/.mcp.json"
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

### Config Reference

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Display name |
| `command` | Stdio only | Binary or command to run |
| `args` | No | Command line arguments |
| `cwd` | No | Working directory |
| `env` | No | Environment variables |
| `transport` | No | `stdio` (default) or `http` |
| `url` | HTTP only | Server endpoint URL |
| `config_path` | No | Path to config file (opened with `e`) |

### AI Chat Providers

The Chat tab reads from `~/.config/mcp-dashboard/ai.json` (auto-created with defaults on first run):

```json
{
  "default_provider": "anthropic",
  "anthropic": {
    "api_key": "",
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 4096
  },
  "openai": {
    "api_key": "",
    "base_url": "https://api.openai.com/v1",
    "model": "gpt-4o",
    "max_tokens": 4096
  },
  "gemini": {
    "api_key": "",
    "model": "gemini-2.0-flash",
    "max_tokens": 4096
  },
  "claude_code": {
    "command": "claude",
    "args": ["--print", "--output-format", "json"]
  },
  "cursor": {
    "command": "cursor",
    "args": ["--chat"]
  }
}
```

**Environment variable fallbacks** -- if `api_key` is empty in the config, these env vars are checked:

| Provider | Environment Variable |
|----------|---------------------|
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Gemini | `GEMINI_API_KEY` |

The `openai.base_url` field supports any OpenAI-compatible endpoint -- point it at Ollama (`http://localhost:11434/v1`), LM Studio, Azure OpenAI, or any other compatible API.

### Auto-Discovery

Servers are automatically discovered from these locations:

| Source | Config Path |
|--------|------------|
| Claude Code | `~/.claude/.mcp.json`, `~/.claude/mcp.json` |
| Cursor | `~/.cursor/mcp.json` |
| Claude Desktop | `~/.config/claude/claude_desktop_config.json` |

Discovered servers appear alongside manual ones with a source badge in the dashboard. Duplicates (same command + args) are automatically deduplicated.

## Keybindings

### Global

| Key | Action |
|-----|--------|
| `1` `2` `3` `4` `5` | Switch tabs |
| `j` / `k` | Navigate list |
| `J` / `K` | Scroll detail panel |
| `r` | Refresh all / reconnect |
| `c` | Toggle connection |
| `e` | Edit config in `$EDITOR` |
| `/` | Search / filter servers |
| `?` | Help overlay |
| `q` | Quit |

### Inspector (Tab 2)

| Key | Action |
|-----|--------|
| `i` | Edit JSON parameters |
| `Enter` | Execute tool |
| `Esc` | Exit input mode |

### Chat (Tab 5)

| Key | Action |
|-----|--------|
| `i` | Enter input mode |
| `Enter` | Send message |
| `Esc` | Exit input / cancel streaming |
| `p` | Cycle AI provider |
| `n` | New conversation |
| `Tab` | Cycle server context |
| `Space` | Toggle server in/out of context |
| `J` / `K` | Scroll messages |

### Search Mode

| Key | Action |
|-----|--------|
| _type_ | Filter by name |
| `Enter` | Keep filter |
| `Esc` | Clear filter |

## How It Works

```
     ┌──────────────┐
     │  Claude API   │
     │  OpenAI API   │◄──── streaming SSE ────┐
     │  Gemini API   │                        │
     └──────────────┘                         │
     ┌──────────────┐                         │
     │ claude --print│◄── subprocess ──┐      │
     │ cursor --chat │                 │      │
     └──────────────┘                  │      │
                        ┌──────────────┴──────┴──┐
                        │      mcp-dashboard      │
                        │  (persistent connections │
                        │   + AI chat + agentic)   │
                        └─────┬───────┬───────┬────┘
                              │       │       │
                     stdio    │  stdio│  HTTP │
                              │       │       │
                        ┌─────▼─┐ ┌───▼───┐ ┌─▼──────────┐
                        │Server │ │Server │ │Remote Server│
                        │  (A)  │ │  (B)  │ │    (C)      │
                        └───────┘ └───────┘ └─────────────┘
```

1. **Connect** -- spawns stdio servers or opens HTTP connections
2. **Initialize** -- MCP handshake with 15s timeout
3. **Discover** -- queries tools, resources, and prompts in parallel
4. **Monitor** -- health checks every 10s on existing connections
5. **Capture** -- streams stderr from child processes in real-time
6. **Detect** -- polls for server death every 500ms, marks as error
7. **Chat** -- stream AI responses, inject MCP context, execute tools on behalf of the AI

Built with [rmcp](https://crates.io/crates/rmcp) (MCP protocol) and [ratatui](https://crates.io/crates/ratatui) (terminal UI).

## Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

```bash
git clone https://github.com/NicksLameCode/mcp-dashboard.git
cd mcp-dashboard
cargo run
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
