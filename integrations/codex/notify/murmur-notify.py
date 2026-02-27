#!/usr/bin/env python3
"""
murmur-notify.py — Codex CLI notify script for Murmur integration.

Records commands from Codex CLI agent turns into Murmur's cross-tool
command history. This enables Murmur to learn from Codex sessions and
provide better completions.

Codex CLI calls this script with a JSON argument after each agent turn.

Install: Add to ~/.codex/config.toml:
    notify = ["/path/to/murmur/integrations/codex/notify/murmur-notify.py"]
"""

import json
import os
import re
import socket
import sys


def send_to_murmur(command: str, cwd: str, exit_code: int = 0) -> None:
    """Send a context/update request to the Murmur daemon."""
    sock_path = os.environ.get("MURMUR_SOCKET", "/tmp/murmur.sock")

    if not os.path.exists(sock_path):
        return

    request = json.dumps({
        "jsonrpc": "2.0",
        "method": "context/update",
        "params": {
            "source": "codex",
            "command": command,
            "cwd": cwd,
            "exit_code": exit_code,
        },
        "id": 1,
    })

    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(2)
        s.connect(sock_path)
        s.send((request + "\n").encode())
        # Read response to avoid broken pipe on daemon side
        s.recv(4096)
        s.close()
    except (ConnectionRefusedError, FileNotFoundError, TimeoutError, OSError):
        pass


def extract_commands(message: str) -> list[tuple[str, int]]:
    """Extract shell commands from an agent's last message.

    Returns a list of (command, exit_code) tuples.
    Looks for common patterns like:
    - ```bash ... ``` code blocks
    - $ command output patterns
    - Ran: command (exit 0) patterns
    """
    commands = []

    # Match bash/shell code blocks
    for match in re.finditer(r"```(?:bash|sh|shell)?\n(.+?)```", message, re.DOTALL):
        block = match.group(1).strip()
        for line in block.split("\n"):
            line = line.strip()
            # Skip comments and empty lines
            if line and not line.startswith("#"):
                # Remove $ prefix if present
                if line.startswith("$ "):
                    line = line[2:]
                commands.append((line, 0))

    return commands


def main() -> None:
    if len(sys.argv) < 2:
        return

    try:
        payload = json.loads(sys.argv[1])
    except (json.JSONDecodeError, IndexError):
        return

    if payload.get("type") != "agent-turn-complete":
        return

    cwd = payload.get("cwd", os.getcwd())
    message = payload.get("last-assistant-message", "")

    if not message:
        return

    # Extract and record commands from the agent's output
    commands = extract_commands(message)
    for command, exit_code in commands:
        send_to_murmur(command, cwd, exit_code)


if __name__ == "__main__":
    main()
