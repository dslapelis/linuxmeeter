#!/usr/bin/env bash
# Run a command against a private, throwaway PipeWire daemon.
#
# The daemon has no hardware devices and no session manager: just a null sink
# standing in for a sound card. It listens on its own socket inside a temp
# runtime dir, so nothing here can see — or disturb — the user's real audio
# session. XDG_CONFIG_HOME is redirected too, so tests never touch
# ~/.config/linuxmeeter.
#
#   scripts/with-test-daemon.sh cargo test -p lm-engine --test audio -- --ignored
set -euo pipefail

if ! command -v pipewire >/dev/null 2>&1; then
    echo "with-test-daemon: 'pipewire' is not installed; cannot run audio tests" >&2
    exit 127
fi

DIR=$(mktemp -d "${TMPDIR:-/tmp}/linuxmeeter-test.XXXXXX")
DAEMON_PID=""

cleanup() {
    if [ -n "$DAEMON_PID" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -rf "$DIR"
}
trap cleanup EXIT INT TERM

mkdir -p "$DIR/run" "$DIR/config/pipewire/pipewire.conf.d"
chmod 700 "$DIR/run"

# Drop-in over the distro's stock pipewire.conf: fixed clock (so metering maths
# are predictable) plus one null sink named alsa_output.* — the engine only
# treats alsa_/bluez_ nodes as real devices.
cat > "$DIR/config/pipewire/pipewire.conf.d/10-linuxmeeter-test.conf" <<'EOF'
context.properties = {
    default.clock.rate        = 48000
    default.clock.quantum     = 1024
    default.clock.min-quantum = 1024
    default.clock.max-quantum = 1024
}
context.objects = [
    { factory = adapter
      args = {
        factory.name     = support.null-audio-sink
        node.name        = "alsa_output.lm-test-sink"
        node.description = "linuxmeeter test sink"
        media.class      = Audio/Sink
        audio.position   = [ FL FR ]
        object.linger    = true
        # No session manager here, so the adapter must configure its own ports.
        adapter.auto-port-config = {
            mode     = dsp
            monitor  = false
            control  = false
            position = preserve
        }
      }
    }
]
EOF

# Client-side counterpart: every stream this process creates (the filter-chain
# modules' capture/playback streams, meter taps, the test tone) also needs to
# configure its own ports, for the same reason.
for conf in client.conf.d client-rt.conf.d; do
    mkdir -p "$DIR/config/pipewire/$conf"
    cat > "$DIR/config/pipewire/$conf/10-linuxmeeter-test.conf" <<'EOF'
stream.properties = {
    adapter.auto-port-config = { mode = dsp, monitor = false, control = false, position = preserve }
}
filter.properties = {
    adapter.auto-port-config = { mode = dsp, monitor = false, control = false, position = preserve }
}
EOF
done

XDG_RUNTIME_DIR="$DIR/run" XDG_CONFIG_HOME="$DIR/config" PIPEWIRE_CORE=lm-test \
    pipewire </dev/null >"$DIR/daemon.log" 2>&1 &
DAEMON_PID=$!

# Wait for the socket rather than sleeping a fixed amount.
for _ in $(seq 100); do
    [ -S "$DIR/run/lm-test" ] && break
    sleep 0.05
done
if [ ! -S "$DIR/run/lm-test" ]; then
    echo "with-test-daemon: daemon failed to start" >&2
    cat "$DIR/daemon.log" >&2
    exit 1
fi

export XDG_RUNTIME_DIR="$DIR/run"
export XDG_CONFIG_HOME="$DIR/config"
export PIPEWIRE_REMOTE=lm-test
export LM_TEST_DAEMON=1

set +e
"$@"
status=$?
set -e

# Surface daemon complaints when the command failed — they usually explain why.
if [ $status -ne 0 ] && [ -s "$DIR/daemon.log" ]; then
    echo "--- test daemon log ---" >&2
    tail -30 "$DIR/daemon.log" >&2
fi
exit $status
