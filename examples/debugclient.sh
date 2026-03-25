#!/bin/bash
# debugclient.sh — TCP client for debugserver
#
# Usage:
#   ./examples/debugclient.sh [port]                              # interactive
#   ./examples/debugclient.sh send [port] '{"cmd":"screenshot"}'  # one-shot send
#
# Interactive mode: persistent TCP connection with shortcuts.
# Send mode: connects, sends one command, prints response, disconnects.
#            Each send is independent — no persistent background process needed.

PORT="${2:-9222}"

case "${1:-interactive}" in
  send)
    PORT="${2:-9222}"
    CMD="$3"
    if [ -z "$CMD" ]; then
      echo "Usage: $0 send [port] '{\"cmd\":\"...\"}'"
      exit 1
    fi
    python3 -c "
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.settimeout(10)
s.connect(('127.0.0.1', int(sys.argv[1])))
s.sendall((sys.argv[2] + '\n').encode())
buf = b''
while b'\n' not in buf:
    chunk = s.recv(65536)
    if not chunk: break
    buf += chunk
print(buf.decode().strip())
s.close()
" "$PORT" "$CMD"
    ;;

  interactive|*)
    PORT="${1:-9222}"
    # If first arg looks like a number, use it as port
    if ! [[ "$PORT" =~ ^[0-9]+$ ]]; then
      PORT=9222
    fi
    PORT="$PORT" python3 << 'PYEOF'
import socket, sys, threading, os

port = int(os.environ.get("PORT", "9222"))

def reader(sock):
    buf = b''
    while True:
        try:
            chunk = sock.recv(65536)
            if not chunk: break
            buf += chunk
            while b'\n' in buf:
                line, buf = buf.split(b'\n', 1)
                text = line.decode()
                # Pretty-print JSON if possible
                try:
                    import json
                    obj = json.loads(text)
                    print(json.dumps(obj, indent=2))
                except:
                    print(text)
                sys.stdout.flush()
        except:
            break
    print('\n[disconnected]')
    os._exit(0)

s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
try:
    s.connect(('127.0.0.1', port))
except ConnectionRefusedError:
    print(f"Cannot connect to 127.0.0.1:{port} — is debugserver running?")
    sys.exit(1)

print(f'Connected to debugserver on port {port}')
print()
print('Commands:')
print('  ss                  screenshot')
print('  f <selector>        find elements         i <selector>   inspect element')
print('  tx <selector>       get text content       a <sel> <attr> get attribute')
print('  c <sel> | c x y     click                  h <sel> | h x y  hover')
print('  k <key>             send key (Enter, Tab, etc.)')
print('  ty <text>           type text              sc <dy>        scroll')
print('  r <width> [height]  resize viewport        nav <url>      navigate')
print('  t [selector]        box tree               q              quit server')
print('  Any JSON: {"cmd":"...","key":"val"}')
print()

t = threading.Thread(target=reader, args=(s,), daemon=True)
t.start()

try:
    while True:
        try:
            line = input('dbg> ').strip()
        except EOFError:
            break
        if not line: continue

        # Shortcuts
        if line == 'ss':
            line = '{"cmd":"screenshot"}'
        elif line == 't':
            line = '{"cmd":"tree"}'
        elif line == 'q':
            line = '{"cmd":"quit"}'
        elif line.startswith('f '):
            sel = line[2:].strip()
            line = '{"cmd":"find","selector":"' + sel + '"}'
        elif line.startswith('i '):
            sel = line[2:].strip()
            line = '{"cmd":"inspect","selector":"' + sel + '"}'
        elif line.startswith('tx '):
            sel = line[3:].strip()
            line = '{"cmd":"text","selector":"' + sel + '"}'
        elif line.startswith('a '):
            parts = line[2:].strip().split(None, 1)
            if len(parts) == 2:
                line = '{"cmd":"attr","selector":"' + parts[0] + '","name":"' + parts[1] + '"}'
            else:
                print("Usage: a <selector> <attribute>")
                continue
        elif line.startswith('c '):
            rest = line[2:].strip()
            parts = rest.split()
            if len(parts) == 2 and parts[0].replace('.','',1).replace('-','',1).isdigit():
                line = '{"cmd":"click","x":' + parts[0] + ',"y":' + parts[1] + '}'
            else:
                line = '{"cmd":"click","selector":"' + rest + '"}'
        elif line.startswith('h '):
            rest = line[2:].strip()
            parts = rest.split()
            if len(parts) == 2 and parts[0].replace('.','',1).replace('-','',1).isdigit():
                line = '{"cmd":"hover","x":' + parts[0] + ',"y":' + parts[1] + '}'
            else:
                line = '{"cmd":"hover","selector":"' + rest + '"}'
        elif line.startswith('k '):
            key = line[2:].strip()
            line = '{"cmd":"key","key":"' + key + '"}'
        elif line.startswith('ty '):
            text = line[3:].strip()
            line = '{"cmd":"type","text":"' + text + '"}'
        elif line.startswith('sc '):
            dy = line[3:].strip()
            line = '{"cmd":"scroll","dy":' + dy + '}'
        elif line.startswith('r '):
            parts = line[2:].strip().split()
            if len(parts) >= 2:
                line = '{"cmd":"resize","width":' + parts[0] + ',"height":' + parts[1] + '}'
            elif len(parts) == 1:
                line = '{"cmd":"resize","width":' + parts[0] + '}'
            else:
                print("Usage: r <width> [height]")
                continue
        elif line.startswith('nav '):
            url = line[4:].strip()
            line = '{"cmd":"navigate","url":"' + url + '"}'
        elif line.startswith('t '):
            sel = line[2:].strip()
            line = '{"cmd":"tree","selector":"' + sel + '"}'
        elif not line.startswith('{'):
            line = '{"cmd":"' + line + '"}'

        s.sendall((line + '\n').encode())
except KeyboardInterrupt:
    pass
finally:
    s.close()
PYEOF
    ;;
esac
