# Murmur Bash Integration
# Add to ~/.bashrc: eval "$(murmur setup bash)"

MURMUR_SOCKET="${MURMUR_SOCKET:-/tmp/murmur.sock}"

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
        echo "$request" | socat - UNIX-CONNECT:"$MURMUR_SOCKET" 2>/dev/null
    elif command -v nc &>/dev/null; then
        echo "$request" | nc -U "$MURMUR_SOCKET" 2>/dev/null
    fi
}

_murmur_complete() {
    if ! _murmur_is_running; then
        return
    fi

    local input="${COMP_LINE}"
    local cursor="${COMP_POINT}"
    local cwd="$PWD"

    local params
    params=$(printf '{"input":"%s","cursor_pos":%d,"cwd":"%s","shell":"bash"}' \
        "$(echo "$input" | sed 's/"/\\"/g')" \
        "$cursor" \
        "$(echo "$cwd" | sed 's/"/\\"/g')")

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
    if 'result' in data and 'items' in data['result']:
        for item in data['result']['items']:
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
