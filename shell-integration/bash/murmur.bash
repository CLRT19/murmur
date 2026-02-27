# Murmur Bash Integration
# Add to ~/.bashrc: eval "$(murmur setup bash)"

MURMUR_SOCKET="${MURMUR_SOCKET:-/tmp/murmur.sock}"
MURMUR_TIMEOUT="${MURMUR_TIMEOUT:-5}"

_murmur_is_running() {
    [[ -S "$MURMUR_SOCKET" ]]
}

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

    if command -v socat &>/dev/null; then
        echo "$request" | timeout "$MURMUR_TIMEOUT" socat - UNIX-CONNECT:"$MURMUR_SOCKET" 2>/dev/null
    elif command -v nc &>/dev/null; then
        echo "$request" | timeout "$MURMUR_TIMEOUT" nc -U "$MURMUR_SOCKET" 2>/dev/null
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

_murmur_trigger() {
    if ! _murmur_is_running; then
        echo ""
        echo "[murmur] daemon not running — start with: murmur start"
        return
    fi

    local input="$READLINE_LINE"
    local cursor="$READLINE_POINT"
    local cwd="$PWD"

    # Skip empty input
    if [[ -z "${input// /}" ]]; then
        return
    fi

    # Escape for JSON (using python3 for correct handling of all special chars)
    local escaped_input escaped_cwd
    escaped_input=$(printf '%s' "$input" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)
    escaped_cwd=$(printf '%s' "$cwd" | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)

    local params
    params="{\"input\":\"$escaped_input\",\"cursor_pos\":$cursor,\"cwd\":\"$escaped_cwd\",\"shell\":\"bash\"}"

    local response
    response=$(_murmur_request "complete" "$params")

    if [[ -z "$response" ]]; then
        return
    fi

    # Parse completions from JSON response
    local completions
    completions=$(echo "$response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'result' in data and 'items' in data['result']:
        for item in data['result']['items']:
            desc = item.get('description', '')
            if desc:
                print(f\"{item['text']}\t({desc})\")
            else:
                print(item['text'])
except:
    pass
" 2>/dev/null)

    if [[ -z "$completions" ]]; then
        return
    fi

    local -a items
    while IFS= read -r line; do
        items+=("$line")
    done <<< "$completions"

    if (( ${#items[@]} == 1 )); then
        # Single completion — insert directly
        local text="${items[0]%%	*}"
        READLINE_LINE="$text"
        READLINE_POINT=${#READLINE_LINE}
    elif (( ${#items[@]} > 1 )); then
        # Multiple completions — display them
        echo ""
        printf '%s\n' "${items[@]}"
        # Insert the first one
        local text="${items[0]%%	*}"
        READLINE_LINE="$text"
        READLINE_POINT=${#READLINE_LINE}
    fi
}

# Bind to Option+Tab (Alt+Tab) — dedicated AI completion key
# Does not conflict with Tab (normal shell completion) or Ctrl+Space (macOS input switch)
bind -x '"\e\t": _murmur_trigger'
