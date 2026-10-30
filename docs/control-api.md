# Local control API

The running Linuxmeeter process listens on
`$XDG_RUNTIME_DIR/linuxmeeter/control.sock`. This is a local Unix stream socket,
not a network API. Scripts, key bindings and hardware controllers share the same
engine state and persistence as the mixer GUI; they do not start another engine.

The socket directory is mode `0700` and the socket is `0600`. Processes running as
the same user can change mixer state. There is no additional authentication.
Linuxmeeter rejects symlink directories and unrelated files at the socket path,
refuses to replace a live server, and recovers an abandoned socket after a crash.
If the socket cannot start (including when `XDG_RUNTIME_DIR` is missing), the
mixer logs a warning and continues without the API.

## Framing and replies

Open a connection, write one UTF-8 JSON request terminated by a newline, and read
one newline-terminated JSON reply. The server then closes the connection. Request
IDs are unsigned 64-bit integers chosen by the client; separate clients may reuse
an ID because the bridge assigns an internal ID to each in-flight request.

```json
{"id":1,"command":{"op":"adjust_gain","target":{"strip":3},"deltaDb":1}}
```

Success has the same `id`, `ok: true` and a `state` object using the existing
camelCase `AppState` representation. This is a snapshot after the command has
executed on the engine thread, rather than confirmation that it was merely queued.
It does not guarantee that hardware has already rendered the new audio or that a
debounced profile save has reached disk. `get_state` is useful for discovering the
current strip IDs, bus IDs, labels and routes; do not assume every profile uses the
same IDs. As in the existing UI state, non-finite silence values serialize as null.

An engine/validation error preserves the parsed request ID:

```json
{"id":1,"ok":false,"error":"unknown strip 999"}
```

A malformed frame or a connection rejected before parsing uses `id: 0` with
`ok: false` and an error. Unknown fields, invalid operations, ambiguous targets,
unknown channels and non-finite input gains are rejected.

## Commands

Targets are either `{"strip": ID}` or `{"bus": "A1"}`. Bus IDs use the existing
`A1`, `A2`, `B1`, `B2` spelling. Use exactly one target field.

| `op` | Other fields | Meaning |
| --- | --- | --- |
| `get_state` | none | Read the current engine state |
| `set_gain` | `target`, `db` | Set gain in dB |
| `adjust_gain` | `target`, `deltaDb` | Add a dB increment to current gain |
| `set_mute` | `target`, `mute` | Set mute to a boolean |
| `toggle_mute` | `target` | Toggle current mute |
| `set_solo` | `strip`, `solo` | Set a strip's solo flag |
| `toggle_solo` | `strip` | Toggle a strip's current solo flag |
| `set_route` | `strip`, `bus`, `on` | Set a routing connection |
| `toggle_route` | `strip`, `bus` | Toggle the current routing connection |

Gains are clamped to the existing fader range, `-72..+12` dB. Relative gain and
all toggles resolve on the single engine thread, so overlapping turns do not lose
updates through stale client snapshots. Mute/solo/route changes follow the normal
GUI state-update path. Gain snapshots are coalesced at 30 Hz, independently of
whether GUI metering is enabled. The socket itself is request/reply; this proposal
does not add subscriptions, meter streaming, DSP controls, device changes or
profile switching.

## Limits and timeouts

Requests are limited to 16 KiB including the newline. The server permits at most
eight active client handlers. Each frame has a 500 ms read deadline; engine
acknowledgements have a two-second timeout, and replies have a one-second write
timeout. Socket I/O runs on worker threads, not the PipeWire thread.

A timeout or disconnect after submitting a command is an indeterminate outcome:
the engine may still execute the command. Read state before deciding what to do;
do not automatically retry a relative increment or toggle. Client IDs correlate
replies within a connection and are not deduplication/idempotency keys.

## Client example

Only Python 3 and its standard library are required:

```sh
# Discover channels without changing anything.
python3 scripts/control-client.py

# Replace 3 with a strip ID returned by get_state.
python3 scripts/control-client.py '{"op":"adjust_gain","target":{"strip":3},"deltaDb":1}'
python3 scripts/control-client.py '{"op":"set_mute","target":{"bus":"A1"},"mute":true}'
```

`scripts/control-client.py` also exposes `request(command, path=None, request_id=1)`
for other scripts. It checks the reply, reports API errors and does not retry.

## Tests

```sh
make test          # protocol/transport unit tests and frontend state tests
make test-control  # socket -> production engine -> reply and GUI-state events
make test-audio    # existing real-signal DSP tests
```

`make test-control` additionally needs Python 3 and the existing audio-test runtime
dependencies. It runs `control_harness` through `scripts/with-test-daemon.sh` with
an isolated runtime directory, profile directory and PipeWire socket. The harness
refuses to run unless `LM_TEST_DAEMON=1`. No user audio devices or profiles are used.

The end-to-end checks cover concurrent relative changes with reused client IDs,
GUI state updates with metering disabled, setters/toggles, gain bounds, invalid
requests, socket permissions and removal of the socket and `lm.*` nodes on exit.
