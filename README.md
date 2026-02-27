# Murmur

AI-powered terminal autocomplete with voice input. Works with Claude Code, Codex CLI, and any shell.

## What is Murmur?

Murmur is a terminal productivity tool that brings AI-powered completions and voice input to your shell. It runs as a lightweight daemon that communicates with your shell via Unix sockets, keeping shell startup fast (<50ms) while providing intelligent suggestions powered by state-of-the-art language models.

### Key Features

- **AI-Powered Autocomplete** — Context-aware command suggestions using Claude Haiku, Codestral, or local models via Ollama
- **Voice Input** — Speak commands naturally; Murmur transcribes and converts them to shell commands or prose
- **Multi-Shell Support** — Native integration with zsh, bash, and fish
- **Multi-LLM Routing** — Automatically picks the right model for the task (Codestral for code, Haiku for shell commands)
- **Rich Context** — Uses shell history, git state, project type, environment variables, and man pages for better suggestions
- **AI Tool Integration** — Native plugins for Claude Code and Codex CLI
- **Fast** — Sub-200ms end-to-end latency with LRU caching, speculative pre-fetching, and smart debouncing
- **Private** — On-device speech-to-text (Apple SpeechAnalyzer / Whisper), no audio leaves your machine

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
| Speech-to-text | Apple SpeechAnalyzer (macOS) / Whisper | Deepgram (cloud) |

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
# ~/.config/murmur/config.toml
[providers.anthropic]
api_key = "sk-ant-..."
model = "claude-haiku-4-5-20251001"
```

4. **Start typing** — press Tab for AI-powered suggestions.

5. **Voice input** (optional):

```bash
# Hold the hotkey (default: Ctrl+Shift+V) and speak
murmur voice test  # Verify microphone works
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
engine = "whisper"  # or "apple" on macOS
hotkey = "ctrl+shift+v"
language = "en"

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

## Claude Code Integration

Murmur can learn from your Claude Code sessions:

```toml
# ~/.claude/settings.json hooks
[hooks.PreToolUse]
command = "murmur hook pre-tool"

[hooks.PostToolUse]
command = "murmur hook post-tool"
```

## Project Structure

```
murmur/
├── Cargo.toml                       # Workspace root
├── crates/
│   ├── murmur-daemon/               # Core daemon
│   ├── murmur-cli/                  # CLI interface
│   ├── murmur-context/              # Context collection
│   ├── murmur-providers/            # LLM provider abstraction
│   ├── murmur-voice/                # Voice input engine
│   └── murmur-protocol/             # Shared JSON-RPC types
├── shell-integration/
│   ├── zsh/murmur.zsh
│   ├── bash/murmur.bash
│   └── fish/murmur.fish
├── integrations/
│   ├── claude-code/
│   └── codex/mcp-server/
└── swift-helpers/                   # macOS SpeechAnalyzer wrapper
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
