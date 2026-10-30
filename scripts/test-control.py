#!/usr/bin/env python3
"""End-to-end control tests. Run with `make test-control`, never against user audio."""
import concurrent.futures
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

spec = importlib.util.spec_from_file_location('control_client', Path(__file__).with_name('control-client.py'))
client = importlib.util.module_from_spec(spec)
spec.loader.exec_module(client)


def main():
    if os.environ.get('LM_TEST_DAEMON') != '1' or os.environ.get('PIPEWIRE_REMOTE') != 'lm-test':
        raise SystemExit('Run with make test-control (private PipeWire daemon required)')
    events = []
    stderr = []
    process = subprocess.Popen([sys.argv[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True)

    def drain_events():
        for line in process.stdout:
            events.append(json.loads(line)['state'])

    def drain_errors():
        stderr.extend(process.stderr)

    thread = threading.Thread(target=drain_events)
    errors = threading.Thread(target=drain_errors)
    thread.start()
    errors.start()
    path = Path(os.environ['XDG_RUNTIME_DIR']) / 'linuxmeeter/control.sock'
    passed = []

    def send(op, **kwargs):
        return client.request({'op': op, **kwargs}, path)

    def raw(payload):
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(4)
            connection.connect(str(path))
            connection.sendall(payload)
            with connection.makefile('rb') as reader:
                return json.loads(reader.readline())

    try:
        deadline = time.monotonic() + 10
        while True:
            try:
                state = send('get_state')
                break
            except (OSError, RuntimeError):
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise
                time.sleep(.05)
        assert path.stat().st_mode & 0o777 == 0o600
        assert path.parent.stat().st_mode & 0o777 == 0o700
        passed.append('private directory/socket and engine readiness')
        strip = next(s for s in state['strips'] if s['kind'] == 'virtual')['id']
        bus = state['buses'][0]['id']
        target = {'strip': strip}

        def channel(snapshot):
            return next(s for s in snapshot['strips'] if s['id'] == strip)

        send('set_gain', target=target, db=-20)

        def adjust(_):
            # Every client deliberately reuses id=1 to exercise bridge correlation.
            for _ in range(10):
                send('adjust_gain', target=target, deltaDb=.25)

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(adjust, range(4)))
        assert channel(send('get_state'))['gainDb'] == -10
        passed.append('40 concurrent relative changes accumulate with reused client IDs')
        deadline = time.monotonic() + 2
        while not any(channel(s)['gainDb'] == -10 for s in list(events)):
            assert time.monotonic() < deadline, 'No state event while metering is disabled'
            time.sleep(.02)
        passed.append('gain updates reach GUI subscribers with metering disabled')
        assert channel(send('set_mute', target=target, mute=True))['mute']
        assert not channel(send('toggle_mute', target=target))['mute']
        assert channel(send('set_solo', strip=strip, solo=True))['solo']
        assert not channel(send('toggle_solo', strip=strip))['solo']
        assert channel(send('set_route', strip=strip, bus=bus, on=True))['routes'][bus]
        assert not channel(send('toggle_route', strip=strip, bus=bus))['routes'][bus]
        passed.append('absolute setters and toggles for mute, solo and routing')
        assert channel(send('set_gain', target=target, db=1000))['gainDb'] == 12
        assert channel(send('set_gain', target=target, db=-1000))['gainDb'] == -72
        snapshot = send('set_gain', target={'bus': bus}, db=-6)
        assert next(b for b in snapshot['buses'] if b['id'] == bus)['gainDb'] == -6
        passed.append('strip/bus gain controls and fader limits')
        for payload in [b'not json\n', b'{"id":1,"command":{"op":"shutdown"}}\n',
                        b'{"id":1,"command":{"op":"get_state","unexpected":1}}\n']:
            assert not raw(payload)['ok']
        for command in [{'op': 'toggle_mute', 'target': {'strip': 9999}},
                        {'op': 'toggle_route', 'strip': 9999, 'bus': bus}]:
            try:
                client.request(command, path)
            except RuntimeError as error:
                assert 'unknown strip' in str(error)
            else:
                raise AssertionError('Unknown strip accepted')
        passed.append('malformed commands and unknown targets rejected')
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise
        thread.join(timeout=2)
        errors.join(timeout=2)
        if process.returncode:
            print(''.join(stderr), file=sys.stderr)
        assert process.returncode == 0, process.returncode
        assert not path.exists(), 'Socket was not removed on shutdown'
    graph = json.loads(subprocess.check_output(['pw-dump'], text=True))
    assert not any(item.get('info', {}).get('props', {}).get('node.name', '').startswith('lm.') for item in graph)
    passed.append('clean shutdown removes the socket and all lm.* audio nodes')
    print(json.dumps({'passed': passed}, indent=2))


if __name__ == '__main__':
    main()
