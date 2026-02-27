# Murmur MCP Server

An [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) server that exposes Murmur's functionality as tools for AI coding assistants. Works with Codex CLI, Claude Code, and any MCP-compatible client.

## Tools

| Tool | Description |
|------|-------------|
| `murmur_complete` | Get AI-powered shell command completions |
| `murmur_status` | Get daemon status, active providers, cache and history counts |
| `murmur_record_command` | Record a command into cross-tool history |
| `murmur_get_history` | Query cross-tool command history |

## Installation

```bash
# From the Murmur repo root
cargo install --path integrations/codex/mcp-server

# Or build without installing
cargo build --release -p murmur-mcp
# Binary at: target/release/murmur-mcp
```

## Setup

### Codex CLI

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.murmur]
command = "murmur-mcp"
enabled = true
```

### Claude Code

Add to `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "murmur": {
      "command": "murmur-mcp"
    }
  }
}
```

### Prerequisites

The Murmur daemon must be running for the MCP tools to work:

```bash
murmur start
```

## Protocol

The server communicates via JSON-RPC 2.0 over stdio (stdin/stdout), implementing the MCP specification version `2025-11-25`. Logs go to stderr.

## Example

```bash
# Test the MCP server manually
echo '{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}},"id":1}
{"jsonrpc":"2.0","method":"tools/list","params":{},"id":2}' | murmur-mcp 2>/dev/null
```
