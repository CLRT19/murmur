# Murmur ZSH Integration
# Add to ~/.zshrc: eval "$(murmur setup zsh)"

# Socket path (matches daemon config)
MURMUR_SOCKET="${MURMUR_SOCKET:-/tmp/murmur.sock}"

# Check if daemon is running
_murmur_is_running() {
    [[ -S "$MURMUR_SOCKET" ]]
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

    # Send request via socat (available on most systems) or nc
    if command -v socat &>/dev/null; then
        echo "$request" | socat - UNIX-CONNECT:"$MURMUR_SOCKET" 2>/dev/null
    elif command -v nc &>/dev/null; then
        echo "$request" | nc -U "$MURMUR_SOCKET" 2>/dev/null
    fi
}

# ZLE widget: AI-powered completion
_murmur_complete() {
    if ! _murmur_is_running; then
        # Fall back to default completion
        zle expand-or-complete
        return
    fi

    local input="$BUFFER"
    local cursor="$CURSOR"
    local cwd="$PWD"

    # Build JSON params
    local params
    params=$(printf '{"input":"%s","cursor_pos":%d,"cwd":"%s","shell":"zsh"}' \
        "$(echo "$input" | sed 's/"/\\"/g')" \
        "$cursor" \
        "$(echo "$cwd" | sed 's/"/\\"/g')")

    # Request completions from daemon
    local response
    response=$(_murmur_request "complete" "$params")

    if [[ -z "$response" ]]; then
        zle expand-or-complete
        return
    fi

    # Parse completion items from JSON response
    local completions
    completions=$(echo "$response" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'result' in data and 'items' in data['result']:
        for item in data['result']['items']:
            desc = item.get('description', '')
            print(f\"{item['text']}\t{desc}\")
except:
    pass
" 2>/dev/null)

    if [[ -z "$completions" ]]; then
        zle expand-or-complete
        return
    fi

    # Use compadd to show completions
    local -a items descriptions
    while IFS=$'\t' read -r text desc; do
        items+=("$text")
        descriptions+=("$desc")
    done <<< "$completions"

    if (( ${#items[@]} == 1 )); then
        # Single completion — insert directly
        BUFFER="${items[1]}"
        CURSOR=${#BUFFER}
        zle redisplay
    else
        # Multiple completions — show menu
        compadd -V murmur -d descriptions -a items
    fi
}

# Register the ZLE widget
zle -N _murmur_complete

# Bind to Tab (keeping original as fallback)
bindkey '^I' _murmur_complete

# Optional: Bind AI completion to a specific key combo (e.g., Ctrl+Space)
# bindkey '^ ' _murmur_complete
