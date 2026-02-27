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
        python3 -c "
import socket, sys
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.settimeout($MURMUR_TIMEOUT)
try:
    sock.connect('$MURMUR_SOCKET')
    sock.sendall(b'$request\n')
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

_murmur_complete() {
    if ! _murmur_is_running; then
        return
    fi

    local input="${COMP_LINE}"
    local cursor="${COMP_POINT}"
    local cwd="$PWD"

    # Skip empty input
    if [[ -z "${input// /}" ]]; then
        return
    fi

    # Escape for JSON
    local escaped_input escaped_cwd
    escaped_input=$(printf '%s' "$input" | sed 's/\\/\\\\/g; s/"/\\"/g')
    escaped_cwd=$(printf '%s' "$cwd" | sed 's/\\/\\\\/g; s/"/\\"/g')

    local params
    params="{\"input\":\"$escaped_input\",\"cursor_pos\":$cursor,\"cwd\":\"$escaped_cwd\",\"shell\":\"bash\"}"

    local response
    response=$(_murmur_request "complete" "$params")

    if [[ -z "$response" ]]; then
        return
    fi

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
            if desc:
                print(f\"{item['text']}\t({desc})\")
            else:
                print(item['text'])
except:
    pass
" 2>/dev/null)

    if [[ -n "$completions" ]]; then
        local -a items
        while IFS= read -r line; do
            items+=("$line")
        done <<< "$completions"
        COMPREPLY=("${items[@]}")
    fi
}

# Register as default completion
complete -D -F _murmur_complete
