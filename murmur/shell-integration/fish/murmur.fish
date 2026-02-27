# Murmur Fish Integration
# Add to ~/.config/fish/config.fish: murmur setup fish | source

set -g MURMUR_SOCKET /tmp/murmur.sock

function _murmur_is_running
    test -S $MURMUR_SOCKET
end

function _murmur_request
    set -l method $argv[1]
    set -l params $argv[2]
    set -l id (random)

    if test -n "$params"
        set -l request "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":$id}"
    else
        set -l request "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":null,\"id\":$id}"
    end

    if command -v socat &>/dev/null
        echo $request | socat - UNIX-CONNECT:$MURMUR_SOCKET 2>/dev/null
    else if command -v nc &>/dev/null
        echo $request | nc -U $MURMUR_SOCKET 2>/dev/null
    end
end

function _murmur_complete
    if not _murmur_is_running
        return
    end

    set -l input (commandline)
    set -l cursor (commandline -C)
    set -l cwd (pwd)

    set -l params (printf '{"input":"%s","cursor_pos":%d,"cwd":"%s","shell":"fish"}' \
        (echo $input | string replace -a '"' '\\"') \
        $cursor \
        (echo $cwd | string replace -a '"' '\\"'))

    set -l response (_murmur_request "complete" $params)

    if test -z "$response"
        return
    end

    echo $response | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'result' in data and 'items' in data['result']:
        for item in data['result']['items']:
            desc = item.get('description', '')
            if desc:
                print(f\"{item['text']}\t{desc}\")
            else:
                print(item['text'])
except:
    pass
" 2>/dev/null
end

# Register completion provider
complete -c '*' -f -a '(_murmur_complete)'
