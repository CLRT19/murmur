# Murmur Fish Integration
# Add to ~/.config/fish/config.fish: murmur setup fish | source

set -g MURMUR_SOCKET /tmp/murmur.sock
set -g MURMUR_TIMEOUT 5

function _murmur_is_running
    test -S $MURMUR_SOCKET
end

function _murmur_request
    set -l method $argv[1]
    set -l params $argv[2]
    set -l id (random)

    set -l request
    if test -n "$params"
        set request "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":$id}"
    else
        set request "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":null,\"id\":$id}"
    end

    if command -v socat &>/dev/null
        echo $request | timeout $MURMUR_TIMEOUT socat - UNIX-CONNECT:$MURMUR_SOCKET 2>/dev/null
    else if command -v nc &>/dev/null
        echo $request | timeout $MURMUR_TIMEOUT nc -U $MURMUR_SOCKET 2>/dev/null
    else
        # Python3 fallback
        python3 -c "
import socket
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
        if b'\\n' in data:
            break
    print(data.decode().strip())
except:
    pass
finally:
    sock.close()
" 2>/dev/null
    end
end

function _murmur_complete
    if not _murmur_is_running
        return
    end

    set -l input (commandline)
    set -l cursor (commandline -C)
    set -l cwd (pwd)

    # Skip empty input
    if test -z (string trim "$input")
        return
    end

    # Escape for JSON
    set -l escaped_input (echo $input | string replace -a '\\' '\\\\' | string replace -a '"' '\\"')
    set -l escaped_cwd (echo $cwd | string replace -a '\\' '\\\\' | string replace -a '"' '\\"')

    set -l params "{\"input\":\"$escaped_input\",\"cursor_pos\":$cursor,\"cwd\":\"$escaped_cwd\",\"shell\":\"fish\"}"

    set -l response (_murmur_request "complete" $params)

    if test -z "$response"
        return
    end

    echo $response | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'error' in data and data['error']:
        pass
    elif 'result' in data and 'items' in data['result']:
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
