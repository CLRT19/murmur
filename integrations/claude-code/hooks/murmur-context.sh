#!/bin/bash
# murmur-context.sh — Claude Code Notification hook
#
# Injects Murmur daemon status as additional context into Claude sessions.
# This runs at the start of Claude Code sessions to inform Claude about
# the available Murmur completions and voice input capabilities.
#
# Install: Add to ~/.claude/settings.json or .claude/settings.json:
# {
#   "hooks": {
#     "Notification": [{
#       "matcher": "murmur",
#       "hooks": [{
#         "type": "command",
#         "command": "/path/to/murmur/integrations/claude-code/hooks/murmur-context.sh"
#       }]
#     }]
#   }
# }

SOCKET="/tmp/murmur.sock"

# Check if daemon is running
if [ ! -S "$SOCKET" ]; then
    echo '{"additionalContext": "Murmur daemon is not running. Shell completions and voice input are unavailable."}'
    exit 0
fi

# Query status from daemon
REQUEST='{"jsonrpc":"2.0","method":"status","params":null,"id":1}'
RESPONSE=""

if command -v socat &>/dev/null; then
    RESPONSE=$(echo "$REQUEST" | socat - UNIX-CONNECT:"$SOCKET" 2>/dev/null)
elif command -v nc &>/dev/null; then
    RESPONSE=$(echo "$REQUEST" | nc -U "$SOCKET" 2>/dev/null)
else
    RESPONSE=$(python3 -c "
import socket, json
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    s.connect('$SOCKET')
    s.send(b'$REQUEST\n')
    data = s.recv(4096).decode()
    s.close()
    print(data)
except:
    print('')
" 2>/dev/null)
fi

if [ -n "$RESPONSE" ]; then
    PROVIDERS=$(echo "$RESPONSE" | python3 -c "import sys, json; r=json.load(sys.stdin).get('result',{}); print(', '.join(r.get('providers_active',[])))" 2>/dev/null)
    CACHE=$(echo "$RESPONSE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('result',{}).get('cache_entries',0))" 2>/dev/null)
    VOICE=$(echo "$RESPONSE" | python3 -c "import sys, json; print(json.load(sys.stdin).get('result',{}).get('voice_enabled',False))" 2>/dev/null)

    echo "{\"additionalContext\": \"Murmur daemon is running. Active providers: ${PROVIDERS:-none}. Cache entries: ${CACHE:-0}. Voice enabled: ${VOICE:-false}. Murmur provides AI-powered shell completions and voice input for this terminal session.\"}"
else
    echo '{"additionalContext": "Murmur daemon appears to be running but did not respond to status query."}'
fi

exit 0
