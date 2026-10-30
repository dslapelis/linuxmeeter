#!/usr/bin/env python3
"""Small local-control client; requires only Python's standard library.

    scripts/control-client.py
    scripts/control-client.py '{"op":"adjust_gain","target":{"strip":3},"deltaDb":1}'
"""
import argparse
import json
import os
from pathlib import Path
import socket


def request(command, path=None, request_id=1):
    if path is None:
        path = Path(os.environ['XDG_RUNTIME_DIR']) / 'linuxmeeter/control.sock'
    payload = json.dumps({'id': request_id, 'command': command}, allow_nan=False).encode() + b'\n'
    if len(payload) > 16 * 1024:
        raise ValueError('Request exceeds 16 KiB')
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(4)
        connection.connect(str(path))
        connection.sendall(payload)
        with connection.makefile('rb') as reader:
            line = reader.readline(1024 * 1024 + 1)
    if not line.endswith(b'\n') or len(line) > 1024 * 1024:
        raise RuntimeError('Missing or oversized control reply')
    reply = json.loads(line)
    # Busy/rejected frames have id=0 because no request has been parsed.
    if not reply.get('ok'):
        raise RuntimeError(reply.get('error', 'Control request failed'))
    if reply.get('id') != request_id:
        raise RuntimeError('Unexpected reply ID')
    return reply['state']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', nargs='?', default='{"op":"get_state"}', help='JSON command')
    args = parser.parse_args()
    try:
        print(json.dumps(request(json.loads(args.command)), indent=2))
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        parser.exit(1, f'{error}\n')


if __name__ == '__main__':
    main()
