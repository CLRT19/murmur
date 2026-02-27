# CLAUDE.md — Murmur Project Guide

## Overview

Murmur is an AI-powered terminal autocomplete + voice input tool written in Rust. It runs as a daemon that communicates with shell plugins via Unix sockets using JSON-RPC 2.0.

## Build & Run

```bash
# Build everything
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run a specific crate's tests
cargo test -p murmur-protocol

# Run clippy
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all

# Run the CLI
cargo run -p murmur-cli -- start
cargo run -p murmur-cli -- stop
cargo run -p murmur-cli -- status
```

## Architecture

- **murmur-protocol** — Shared types (JSON-RPC messages, completion requests/responses). No async, no I/O. Pure data types + serialization.
- **murmur-context** — Collects shell context (history, CWD, git state, env vars, project type). Async where needed (git operations).
- **murmur-providers** — LLM provider abstraction. Each provider implements the `Provider` trait. Includes Anthropic, Codestral, Ollama.
- **murmur-voice** — Audio capture (cpal), speech-to-text (whisper-rs), voice restructuring pipeline.
- **murmur-daemon** — Tokio-based Unix socket server. Routes requests, manages cache, orchestrates context + providers.
- **murmur-cli** — User-facing CLI (clap). Manages daemon lifecycle, shell setup, voice testing.

### Dependency graph (crates depend downward):

```
murmur-cli
    └── murmur-daemon
            ├── murmur-providers
            ├── murmur-context
            ├── murmur-voice
            └── murmur-protocol (shared by all)
```

## Coding Standards

- **Rust edition:** 2021
- **MSRV:** 1.80
- **Error handling:** Use `thiserror` for library errors, `anyhow` in CLI/daemon for application errors
- **Async:** Tokio runtime. Use `async fn` with `#[tokio::main]` in binaries.
- **Serialization:** `serde` + `serde_json` for JSON-RPC, `toml` for config
- **Logging:** `tracing` crate (not `log`)
- **CLI parsing:** `clap` with derive macros
- **Tests:** Place unit tests in the same file (`#[cfg(test)] mod tests`). Integration tests in `tests/`.

## Conventions

- Keep shell integration scripts minimal. All logic belongs in the daemon.
- Provider implementations must be non-blocking. Use `reqwest` for HTTP calls.
- Cache keys should include the full context hash, not just the command prefix.
- Config lives at `~/.config/murmur/config.toml`. Socket at `/tmp/murmur.sock`. PID at `/tmp/murmur.pid`.
- All public types in murmur-protocol must derive `Serialize, Deserialize, Debug, Clone`.

## File Paths

- Workspace root: `Cargo.toml`
- Crates: `crates/murmur-{daemon,cli,context,providers,voice,protocol}/`
- Shell scripts: `shell-integration/{zsh,bash,fish}/`
- CI: `.github/workflows/ci.yml`
- Config example: `config.example.toml`
