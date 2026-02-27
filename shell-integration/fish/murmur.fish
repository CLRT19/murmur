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
        # Python3 fallback — pass request via env var to avoid injection
        set -x MURMUR_REQ $request
        set -x MURMUR_SOCK $MURMUR_SOCKET
        set -x MURMUR_TMO $MURMUR_TIMEOUT
        python3 -c "
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
        if b'\\n' in data:
            break
    print(data.decode().strip())
except:
    pass
finally:
    sock.close()
" 2>/dev/null
        set -e MURMUR_REQ
        set -e MURMUR_SOCK
        set -e MURMUR_TMO
    end
end

function _murmur_trigger
    if not _murmur_is_running
        echo "[murmur] daemon not running — start with: murmur start"
        commandline -f repaint
        return
    end

    set -l input (commandline)
    set -l cursor (commandline -C)
    set -l cwd (pwd)

    # Skip empty input
    if test -z (string trim "$input")
        return
    end

    # Escape for JSON (using python3 for correct handling of all special chars)
    set -l escaped_input (printf '%s' $input | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)
    set -l escaped_cwd (printf '%s' $cwd | python3 -c "import sys,json; print(json.dumps(sys.stdin.read())[1:-1])" 2>/dev/null)

    set -l params "{\"input\":\"$escaped_input\",\"cursor_pos\":$cursor,\"cwd\":\"$escaped_cwd\",\"shell\":\"fish\"}"

    set -l response (_murmur_request "complete" $params)

    if test -z "$response"
        return
    end

    # Parse completions and insert the first one
    set -l completion (echo $response | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'result' in data and 'items' in data['result']:
        items = data['result']['items']
        if items:
            print(items[0]['text'])
except:
    pass
" 2>/dev/null)

    if test -n "$completion"
        commandline -r -- $completion
        commandline -C (string length "$completion")
    end
end

# Bind to Option+Tab (Alt+Tab) — dedicated AI completion key
# Does not conflict with Tab (normal shell completion) or Ctrl+Space (macOS input switch)
bind \e\t _murmur_trigger
