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
TIMEOUT=3

# Check if daemon is running
if [ ! -S "$SOCKET" ]; then
    echo '{"additionalContext": "Murmur daemon is not running. Shell completions and voice input are unavailable."}'
    exit 0
fi

# Portable timeout wrapper
_timeout() {
    if command -v timeout &>/dev/null; then
        timeout "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$@"
    else
        shift
        "$@"
    fi
}

# Query status from daemon
REQUEST='{"jsonrpc":"2.0","method":"status","params":null,"id":1}'
RESPONSE=""

if command -v socat &>/dev/null; then
    RESPONSE=$(echo "$REQUEST" | _timeout "$TIMEOUT" socat - UNIX-CONNECT:"$SOCKET" 2>/dev/null)
elif command -v nc &>/dev/null; then
    RESPONSE=$(echo "$REQUEST" | _timeout "$TIMEOUT" nc -U "$SOCKET" 2>/dev/null)
else
    RESPONSE=$(MURMUR_REQ="$REQUEST" MURMUR_SOCK="$SOCKET" python3 -c "
import socket, os
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(3)
try:
    s.connect(os.environ['MURMUR_SOCK'])
    s.send((os.environ['MURMUR_REQ'] + '\n').encode())
    data = s.recv(4096).decode()
    s.close()
    print(data)
except:
    print('')
" 2>/dev/null)
fi

if [ -n "$RESPONSE" ]; then
    # Build JSON output safely via python3 to avoid interpolation issues
    echo "$RESPONSE" | python3 -c "
import sys, json
try:
    r = json.load(sys.stdin).get('result', {})
    providers = ', '.join(r.get('providers_active', [])) or 'none'
    cache = r.get('cache_entries', 0)
    voice = r.get('voice_enabled', False)
    msg = f'Murmur daemon is running. Active providers: {providers}. Cache entries: {cache}. Voice enabled: {voice}. Murmur provides AI-powered shell completions and voice input for this terminal session.'
    print(json.dumps({'additionalContext': msg}))
except:
    print(json.dumps({'additionalContext': 'Murmur daemon is running but status could not be parsed.'}))
" 2>/dev/null
else
    echo '{"additionalContext": "Murmur daemon appears to be running but did not respond to status query."}'
fi

exit 0
