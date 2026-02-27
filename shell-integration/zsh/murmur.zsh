# Murmur ZSH Integration
# Add to ~/.zshrc: eval "$(murmur setup zsh)"

# Socket path (matches daemon config)
MURMUR_SOCKET="${MURMUR_SOCKET:-/tmp/murmur.sock}"

# Debounce delay in seconds (completions wait this long after last keystroke)
MURMUR_DEBOUNCE="${MURMUR_DEBOUNCE:-0.3}"

# Request timeout in seconds
MURMUR_TIMEOUT="${MURMUR_TIMEOUT:-5}"

# Check if daemon is running
_murmur_is_running() {
    [[ -S "$MURMUR_SOCKET" ]]
}

# Portable timeout wrapper (macOS may not have GNU timeout)
_murmur_timeout() {
    if command -v timeout &>/dev/null; then
        timeout "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$@"
    else
        # No timeout available — run without it
        shift  # Remove the timeout duration argument
        "$@"
    fi
}

# Send a JSON-RPC request to the daemon and get the response
_murmur_request() {
    local method="$1"
    local params="$2"
    local id=$((RANDOM))

    local request
    if [[ -n "$params" ]]; then
        request="{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":$id}"
    else
        request="{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":null,\"id\":$id}"
    fi

    # Send request via socat (preferred), nc, or python3 (fallback)
    if command -v socat &>/dev/null; then
        echo "$request" | _murmur_timeout "$MURMUR_TIMEOUT" socat - UNIX-CONNECT:"$MURMUR_SOCKET" 2>/dev/null
    elif command -v nc &>/dev/null; then
        echo "$request" | _murmur_timeout "$MURMUR_TIMEOUT" nc -U "$MURMUR_SOCKET" 2>/dev/null
    else
        # Python3 fallback — pass request via env var to avoid injection
        MURMUR_REQ="$request" MURMUR_SOCK="$MURMUR_SOCKET" MURMUR_TMO="$MURMUR_TIMEOUT" python3 -c "
import socket, os
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.settimeout(float(os.environ.get('MURMUR_TMO', '5')))
try:
    sock.connect(os.environ['MURMUR_SOCK'])
    sock.sendall((os.environ['MURMUR_REQ'] + '\n').encode())
    data = b''
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        data += chunk
        if b'\n' in data:
            break
    print(data.decode().strip())
except:
    pass
finally:
    sock.close()
" 2>/dev/null
    fi
}

# Debouncing state
typeset -g _MURMUR_DEBOUNCE_PID=""
typeset -g _MURMUR_LAST_INPUT=""

# ZLE widget: AI-powered completion
_murmur_complete() {
    if ! _murmur_is_running; then
        zle -M "[murmur] daemon not running — start with: murmur start"
        return
    fi

    local input="$BUFFER"
    local cursor="$CURSOR"
    local cwd="$PWD"

    # Skip if input is empty or only whitespace
    if [[ -z "${input// /}" ]]; then
        return
    fi

    # Build JSON params (escape special characters for JSON)
    local escaped_input escaped_cwd
    escaped_input=$(printf '%s' "$input" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)
    escaped_cwd=$(printf '%s' "$cwd" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)

    local params
    params="{\"input\":\"$escaped_input\",\"cursor_pos\":$cursor,\"cwd\":\"$escaped_cwd\",\"shell\":\"zsh\"}"

    # Request completions from daemon
    local response
    response=$(_murmur_request "complete" "$params")

    if [[ -z "$response" ]]; then
        return
    fi

    # Parse completion items from JSON response
    local completions
    completions=$(echo "$response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'error' in data and data['error']:
        pass
    elif 'result' in data and 'items' in data['result']:
        for item in data['result']['items']:
            desc = item.get('description', '')
            print(f\"{item['text']}\t{desc}\")
except:
    pass
" 2>/dev/null)

    if [[ -z "$completions" ]]; then
        return
    fi

    # Use compadd to show completions
    local -a items descriptions
    while IFS=$'\t' read -r text desc; do
        items+=("$text")
        descriptions+=("$desc")
    done <<< "$completions"

    if (( ${#items[@]} == 0 )); then
        return
    fi

    if (( ${#items[@]} == 1 )); then
        # Single completion — insert directly
        BUFFER="${items[1]}"
        CURSOR=${#BUFFER}
        zle redisplay
    else
        # Multiple completions — display as numbered list and insert the first
        local display=""
        local i
        for (( i=1; i<=${#items[@]}; i++ )); do
            display+="  $i) ${items[$i]}"
            if [[ -n "${descriptions[$i]}" ]]; then
                display+="  — ${descriptions[$i]}"
            fi
            display+=$'\n'
        done
        zle -M "$display"
        # Insert the top suggestion
        BUFFER="${items[1]}"
        CURSOR=${#BUFFER}
        zle redisplay
    fi
}

# Register the ZLE widget
zle -N _murmur_complete

# Bind to Option+Tab (Alt+Tab) — dedicated AI completion key
# Does not conflict with Tab (normal shell completion) or Ctrl+Space (macOS input switch)
bindkey '\e\t' _murmur_complete
