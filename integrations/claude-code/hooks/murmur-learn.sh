#!/bin/bash
# murmur-learn.sh — Claude Code PostToolUse hook
#
# Records Bash commands executed by Claude Code into Murmur's cross-tool
# command history. Runs asynchronously to avoid blocking Claude.
#
# Install: Add to ~/.claude/settings.json or .claude/settings.json:
# {
#   "hooks": {
#     "PostToolUse": [{
#       "matcher": "Bash",
#       "hooks": [{
#         "type": "command",
#         "command": "/path/to/murmur/integrations/claude-code/hooks/murmur-learn.sh",
#         "async": true,
#         "timeout": 5
#       }]
#     }]
#   }
# }

INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | python3 -c "import sys, json; print(json.load(sys.stdin).get('tool_name', ''))" 2>/dev/null)

# Only process Bash tool calls
if [ "$TOOL_NAME" != "Bash" ]; then
    exit 0
fi

COMMAND=$(echo "$INPUT" | python3 -c "import sys, json; print(json.load(sys.stdin).get('tool_input', {}).get('command', ''))" 2>/dev/null)
CWD=$(echo "$INPUT" | python3 -c "import sys, json; print(json.load(sys.stdin).get('cwd', '.'))" 2>/dev/null)
EXIT_CODE=$(echo "$INPUT" | python3 -c "import sys, json; print(json.load(sys.stdin).get('tool_response', {}).get('exit_code', 0))" 2>/dev/null)
SESSION_ID=$(echo "$INPUT" | python3 -c "import sys, json; print(json.load(sys.stdin).get('session_id', ''))" 2>/dev/null)

# Skip empty commands
if [ -z "$COMMAND" ]; then
    exit 0
fi

SOCKET="/tmp/murmur.sock"

# Skip if daemon isn't running
if [ ! -S "$SOCKET" ]; then
    exit 0
fi

# Build JSON-RPC request (using python3 for safe JSON encoding)
REQUEST=$(python3 -c "
import json, sys
print(json.dumps({
    'jsonrpc': '2.0',
    'method': 'context/update',
    'params': {
        'source': 'claude-code',
        'command': '''$COMMAND''',
        'cwd': '$CWD',
        'exit_code': int('${EXIT_CODE:-0}'),
        'session_id': '$SESSION_ID'
    },
    'id': 1
}))
" 2>/dev/null)

# Fire-and-forget to Murmur daemon
if command -v socat &>/dev/null; then
    echo "$REQUEST" | socat - UNIX-CONNECT:"$SOCKET" 2>/dev/null &
elif command -v nc &>/dev/null; then
    echo "$REQUEST" | nc -U "$SOCKET" 2>/dev/null &
else
    # Python3 fallback
    python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    s.connect('$SOCKET')
    s.send(b'$REQUEST\n')
    s.recv(4096)
    s.close()
except:
    pass
" 2>/dev/null &
fi

exit 0
