# Murmur

AI-powered terminal autocomplete with voice input. Works with Claude Code, Codex CLI, and any shell.

## What is Murmur?

Murmur is a terminal productivity tool that brings AI-powered completions and voice input to your shell. It runs as a lightweight daemon that communicates with your shell via Unix sockets, keeping shell startup fast (<50ms) while providing intelligent suggestions powered by state-of-the-art language models.

### Key Features

- **AI-Powered Autocomplete** — Context-aware command suggestions using Claude Haiku, Codestral, or local models via Ollama
- **Voice Input** — Speak commands naturally; Murmur transcribes and converts them to shell commands or prose
- **Multi-Shell Support** — Native integration with zsh, bash, and fish
- **Multi-LLM Routing** — Automatically picks the right model for the task (Codestral for code, Haiku for shell commands)
- **Rich Context** — Uses shell history, git state, project type, environment variables for better suggestions
- **AI Tool Integration** — Native plugins for Claude Code and Codex CLI via hooks and MCP
- **Cross-Tool History** — Commands from terminals, Claude Code, and Codex flow into a shared history for smarter completions
- **Fast** — Sub-200ms end-to-end latency with LRU caching, speculative pre-fetching, and smart debouncing
- **Private** — On-device speech-to-text (Apple Speech / Whisper), no audio leaves your machine

## Architecture

```
┌──────────────┐     Unix Socket     ┌──────────────────┐     HTTP/gRPC     ┌─────────────┐
│ Shell Plugin │ ◄──────────────────► │  Murmur Daemon   │ ◄──────────────► │ LLM Provider│
│ (zsh/bash)   │                     │  (Rust/Tokio)    │                   │ (Haiku/etc) │
└──────────────┘                     └────────┬─────────┘                   └─────────────┘
                                              │
                                    ┌─────────┴─────────┐
                                    │                   │
                              ┌─────┴─────┐       ┌────┴──────┐
                              │  Voice    │       │  Context  │
                              │  Engine   │       │  Collector│
                              │ (STT+LLM) │       │ (history, │
                              └───────────┘       │  git, env)│
                                                  └───────────┘
```

Shell plugins are ultra-thin scripts. All intelligence lives in the daemon.

## Model Recommendations

| Use Case | Primary Model | Fallback |
|----------|--------------|----------|
| Shell autocomplete | Claude Haiku 4.5 | Ollama (local) |
| Code completion (FIM) | Codestral 25.01 | DeepSeek-Coder-V2-Lite (local) |
| Voice restructuring | Claude Haiku 4.5 | GPT-4o-mini |
| Speech-to-text | Apple Speech (macOS) / Whisper | Deepgram (cloud) |

## Installation

### From source (requires Rust 1.80+)

```bash
git clone https://github.com/CLRT19/murmur.git
cd murmur
cargo install --path crates/murmur-cli
```

### Homebrew (coming soon)

```bash
brew install murmur
```

## Quick Start

1. **Start the daemon:**

```bash
murmur start
```

2. **Add shell integration** (zsh):

```bash
# Add to your ~/.zshrc
eval "$(murmur setup zsh)"
```

3. **Configure your API key:**

```bash
mkdir -p ~/.config/murmur
cp config.example.toml ~/.config/murmur/config.toml
# Edit the file and set your API key
```

4. **Start typing** — press **Option+Tab** for AI-powered suggestions.

5. **Voice input** (optional):

```bash
murmur voice test --file recording.wav   # Process a WAV file
murmur voice status                       # Check voice engine status
```

## Configuration

Murmur is configured via `~/.config/murmur/config.toml`:

```toml
[daemon]
socket_path = "/tmp/murmur.sock"
cache_size = 1000
log_level = "info"

[providers.anthropic]
api_key = "sk-ant-..."
model = "claude-haiku-4-5-20251001"

[providers.codestral]
api_key = "..."
model = "codestral-latest"
endpoint = "https://codestral.mistral.ai/v1/fim/completions"
enabled = true

[providers.ollama]
endpoint = "http://localhost:11434"
model = "codellama:7b"
enabled = false  # Enable for offline fallback

[voice]
enabled = false
engine = "whisper"  # "whisper", "apple" (macOS), or "deepgram" (cloud)
hotkey = "ctrl+shift+v"
language = "en"
confidence_threshold = 0.5
# deepgram_api_key = "your-key"  # Required for Deepgram cloud STT

[context]
history_lines = 500
git_enabled = true
project_detection = true
```

## Shell Support

| Shell | Status | Integration |
|-------|--------|-------------|
| zsh | Supported | ZLE widget + compadd |
| bash | Supported | Readline binding |
| fish | Supported | Fish completions |

## AI Tool Integration

Murmur integrates with AI coding assistants to share context bi-directionally. Commands executed by AI tools are recorded in Murmur's cross-tool history, which improves future completions.

### Codex CLI

Codex CLI connects to Murmur via an **MCP server** that exposes Murmur's tools.

**1. Install the MCP server:**

```bash
cargo install --path integrations/codex/mcp-server
```

**2. Register with Codex CLI** — add to `~/.codex/config.toml`:

```toml
[mcp_servers.murmur]
command = "murmur-mcp"
enabled = true
```

This gives Codex access to these tools:
- `murmur_complete` — Get AI-powered shell completions
- `murmur_status` — Check daemon status and providers
- `murmur_record_command` — Record command executions
- `murmur_get_history` — Query cross-tool command history

**3. (Optional) Enable notify script** — records Codex agent commands into Murmur's history:

```toml
# Add to ~/.codex/config.toml
notify = ["/path/to/murmur/integrations/codex/notify/murmur-notify.py"]
```

### Claude Code

Claude Code connects to Murmur via **hooks** that fire on tool use events, and optionally via the same MCP server.

**Option A — Hooks (records commands from Claude Code sessions):**

Add to `.claude/settings.json` or `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/murmur/integrations/claude-code/hooks/murmur-learn.sh",
            "async": true,
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

**Option B — MCP server (gives Claude Code access to Murmur tools):**

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

### Cross-Tool History

When either integration is active, Murmur builds a unified command history across all tools:

```bash
murmur status   # Shows history_entries count
```

The daemon exposes two JSON-RPC methods for this:
- `context/update` — Record a command (used by hooks and MCP tools)
- `history/list` — Query history with optional cwd filter

## CLI Commands

```bash
murmur start [--foreground] [--config path]   # Start the daemon
murmur stop                                    # Stop the daemon
murmur status                                  # Show daemon status
murmur setup <shell>                           # Print shell integration script
murmur doctor                                  # Run diagnostic checks
murmur voice test [--file <wav>] [--mode cmd]  # Test voice input
murmur voice status                            # Show voice engine status
```

## Project Structure

```
murmur/
├── Cargo.toml                       # Workspace root
├── crates/
│   ├── murmur-daemon/               # Core daemon (server, cache, routing)
│   ├── murmur-cli/                  # CLI interface
│   ├── murmur-context/              # Context collection (history, git, env)
│   ├── murmur-providers/            # LLM providers (Anthropic, Codestral, Ollama)
│   ├── murmur-voice/                # Voice engine (STT, restructuring)
│   └── murmur-protocol/             # Shared JSON-RPC types
├── shell-integration/
│   ├── zsh/murmur.zsh
│   ├── bash/murmur.bash
│   └── fish/murmur.fish
├── integrations/
│   ├── claude-code/                 # Claude Code hooks
│   │   ├── hooks/murmur-learn.sh    # PostToolUse → records commands
│   │   ├── hooks/murmur-context.sh  # Injects Murmur status as context
│   │   └── settings.example.json
│   └── codex/                       # Codex CLI integration
│       ├── mcp-server/              # MCP server binary (murmur-mcp)
│       ├── notify/murmur-notify.py  # Notify script → records commands
│       └── config.example.toml
└── swift-helpers/                   # macOS Apple Speech STT wrapper
```

## Roadmap

- [x] Phase 1: Foundation + MVP autocomplete (zsh + Claude Haiku)
- [x] Phase 2: Multi-provider with failover, Codestral, pre-fetching, improved routing
- [x] Phase 3: Voice input (Deepgram, Apple Speech, LLM restructuring, CLI commands)
- [x] Phase 4: Claude Code hooks + MCP server + cross-tool command history
- [ ] Phase 5: Polish + distribution (Homebrew, AUR)

## Contributing

Contributions are welcome! Please read the [CLAUDE.md](CLAUDE.md) for project conventions and coding standards.

## License

MIT License. See [LICENSE](LICENSE) for details.
