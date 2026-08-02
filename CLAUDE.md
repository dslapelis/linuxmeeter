# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A VoiceMeeter-style virtual audio mixer for Linux: Tauri 2 shell + Svelte 5 frontend +
a Rust PipeWire engine. Input strips (hardware capture or virtual sinks apps play into)
run gate → comp → EQ, feed a routing matrix, and land on four buses (A1/A2 → hardware
sinks, B1/B2 → virtual microphones). DSP is the LSP LV2 plugins hosted inside
`libpipewire-module-filter-chain`.

## Commands

Use the Makefile; it is the canonical entry point. `make` with no target lists everything.

```sh
make install      # frontend deps (pnpm install)
make dev          # browser-only UI against the MOCK backend (design iteration)
make app          # full app against the live PipeWire graph
make test         # Rust + frontend unit tests (no audio system needed)
make test-audio   # integration tests against a private throwaway PipeWire daemon
make check        # svelte-check + cargo check --workspace --all-targets
make build        # release binary -> target/release/linuxmeeter
make graph        # headless REPL: full topology — `route a1 0|1`, `links`, `meters 1`, `q`
make spike        # headless REPL: one strip, raw param pokes — `set comp:cr 8`, `vol 0.5`, `q`
```

Also available: `test-rust` / `test-ui` (the halves of `test`), `test-all`, `check-ui` /
`check-rust`, `clippy`, `fmt`, `fmt-check`, `run` (already-built release binary), `clean`,
and `clean-cache` (wipes the WebKitGTK cache — see gotchas below).

`LOG` sets `RUST_LOG` for `app`, `graph`, `spike`, and `run`; it defaults to `info`, so
`make app LOG=debug` or `make graph LOG=lm_engine=debug` for tracing detail. `PNPM` and
`CARGO` are overridable the same way.

## Testing

Three layers, in ascending order of what they need:

**`make test` — pure logic, no audio system.** Rust unit tests live in `#[cfg(test)]`
modules beside the code (`params`, `links`, `filterchain`, `meter`, `persist`,
`lm-protocol`); frontend tests are `src/**/*.test.ts` under vitest. Between them they
pin the things that are silently wrong rather than loudly broken: Props pod shapes,
`match_ports` channel pairing, `state.restore-props` on every node, profile migration,
meter dBFS arithmetic, the camelCase IPC contract with `types.ts`, the fader taper, and
the EQ response curve.

**`make test-audio` — real PipeWire, real LSP plugins, real samples.**
`scripts/with-test-daemon.sh` starts a *private* daemon: its own socket in a temp runtime
dir, one null sink, no session manager, `XDG_CONFIG_HOME` redirected so profiles never
touch `~/.config/linuxmeeter`. `crates/lm-engine/tests/audio.rs` then builds the
production signal path out of the engine's own parts — tone → strip (gate/comp/EQ) →
routing matrix → bus (limiter) → sink, with meter taps — and asserts on measured dBFS.
A 0.5-amplitude sine must arrive at −6.02 dBFS peak / −9.03 RMS; `-12 dB` of gain must
move the meter by 12 dB; mute must reach the noise floor; a gate threshold above the
signal must close it. This is what catches "the knob does nothing" bugs.

These tests are `#[ignore]`d and additionally refuse to run unless `LM_TEST_DAEMON=1` is
set by the harness script — a bare `cargo test -- --ignored` fails fast rather than
creating real devices in the developer's own audio session. Because they share one
daemon and one process environment, they run with `--test-threads=1`.

Without a session manager nothing configures ports, so the harness sets
`adapter.auto-port-config` for both the daemon's null sink and this process's streams.
Without that, every node appears with zero ports and no link can ever form.

**Manual.** Perceptual quality, real hardware enumeration, and WirePlumber policy
interactions are still eyes-and-ears work: `make graph` / `make spike` plus `pw-dump`,
`pw-link`, `pw-top`, `pw-record --target lm.bus.b1 out.wav`.

## Architecture

Three layers, each with a hard boundary:

- **`crates/lm-protocol`** — every type crossing a boundary (`AppState`, `StripState`,
  `EngineCommand`, `EngineEvent`, `MeterFrame`). No dependencies beyond serde.
- **`crates/lm-engine`** — all PipeWire code. Deliberately Tauri-free so it runs headless
  under `examples/`.
- **`src-tauri`** — thin shell: `#[tauri::command]` wrappers that forward to the engine,
  an event-pump thread, tray icon, autostart, close-to-tray.
- **`src/`** — Svelte 5 (runes) frontend, vanilla CSS.

### The engine thread

`lm_engine::engine::spawn()` starts one thread owning a PipeWire main loop. It is the
sole owner of all audio state. Communication is strictly:

- in: `pipewire::channel::Sender<EngineCommand>` (attached to the loop)
- out: `std::sync::mpsc::Receiver<EngineEvent>` → pumped into webview events
  (`state_changed`, `meters`) by a thread in `src-tauri/src/main.rs`

Never reach into engine internals from the Tauri side; add an `EngineCommand` variant.

### Graph construction

The graph is built once, after the registry's initial enumeration completes
(`core.add_listener_local().done(...)` → `build_graph`). Each strip and bus is one
in-process `libpipewire-module-filter-chain` module loaded via the FFI shim in
`filterchain.rs` (`LoadedModule` — pipewire-rs 0.10 has no safe wrapper). Dropping a
`LoadedModule` unloads it and removes its nodes.

**Cleanliness invariant: `pw-dump` must show zero `lm.*` nodes after the app exits.**
That is why modules are loaded into our own context and why `RunEvent::Exit` sends
`Shutdown` and sleeps briefly before the process dies.

Node naming is the stable identity used everywhere (profiles, routes, links) — see the
`node_base()` / `control_node()` / `out_node()` / `in_node()` / `tap_node()` helpers on
`StripState` and `BusState` in lm-protocol. Never key anything on transient PipeWire
global ids.

Changing a strip's capture device or an A-bus's target is a **module reload**
(`reload_strip` / `reload_bus`): node names stay identical, so the LinkManager's desired
routes and the meter tap re-link automatically as new ports appear.

### Routing (`links.rs`)

`LinkManager` holds a set of desired `(output node name, input node name)` routes and
reconciles them against the live `GraphModel` on every relevant registry event. It
computes the full wanted set and diffs — it does not apply incremental deltas, because
port arrival is racy. Links are created with `object.linger=false`, so dropping the proxy
tears the link down.

`match_ports` is channel-strict (FL→FL) with two deliberate fallbacks: index pairing when
both sides report `UNK` (streams before format negotiation), and mono fan-out only for a
*single*-port output. Do not loosen the fan-out rule — a stereo node briefly has one port
during arrival, and fanning out then cross-links FL→FR.

### Parameters (`params.rs`)

All runtime control is one mechanism: `node.set_param(ParamType::Props, ...)` with a
libspa pod. Filter-chain nodes expose unlinked plugin control ports as
`"<plugin>:<port>" <float>` pairs in the Props `params` struct; volume/mute use the
standard `channelVolumes` / `mute` keys on the same node.

**LSP thresholds and gains are LINEAR, not dB** — always route through
`params::db_to_linear`. Values are pushed to the node named by `control_node()`, which is
the capture-side node (`.cap` for hardware strips, the sink node itself for virtual).

Every `lm.*` node sets `state.restore-props: false`; without it WirePlumber's
restore-stream overwrites our volumes and mutes as nodes appear.

### Metering

One `MeterTap` per metered point — a passive, non-autoconnecting F32 capture stream
(`meter.rs`) linked by our own `LinkManager` like any other route, so metering needs no
session-manager cooperation. The process callback accumulates peak + sum-of-squares into
atomics; a 30 Hz timer drains all taps into one batched `MeterFrame`.

The tap declares **stereo explicitly** in its requested format. Ports are created from
the requested format, so a tap that leaves the channel count open has no ports until
something negotiates one for it — and with autoconnect off, nothing ever does. That
declaration is what makes metering genuinely independent of the session manager.

On the frontend, **meter data never touches Svelte reactive state**. Frames land in a
plain `Map` in `src/lib/state/meters.ts`; a single shared `requestAnimationFrame` loop
advances ballistics (attack/decay, 1.5 s peak hold, clip latch) and calls each registered
canvas renderer. Keep it that way — routing 30 fps × N strips through runes destroys
frame time.

**The render loop is idle-driven, and that is load-bearing.** Each meter is its own
canvas, so every repaint is a Skia GPU pass; drawing all of them unconditionally at
display refresh cost ~45% of a core by itself. Four rules keep it cheap, and all four
are needed:

- capped at `MAX_FPS` (30, matching the engine's drain rate — anything faster only
  interpolates between frames that already arrived);
- only meters whose displayed values actually moved are redrawn (`MeterState.dirty`);
- when a tick draws nothing the loop *stops*, and `ingestFrame` / `markDirty` restart it;
- it does not run while `document.hidden`.

Movement is judged against `EPSILON_DB` (0.1 dB — sub-pixel at the rendered scale), which
is what stops noise-floor jitter from waking the loop on a silent mixer. The peak and
peak-hold attacks compare with `>=`, not `>`: with `>` a steady tone decays one frame and
re-attacks the next, flickering forever and never letting the loop settle.

Anything that clears a canvas outside the loop (backing-store resize, coming back from
off-screen) owes a `markDirty(key)` — the loop will not repaint it otherwise.

Symmetrically on the backend: `EngineCommand::SetMetersEnabled` gates the 30 Hz drain,
and `src-tauri/src/main.rs` turns it off whenever the window is hidden. Every emitted
frame costs a `webkit_web_view_run_javascript` (Tauri emits events by compiling a fresh
script string), so a mixer parked in the tray should not be producing them. Audio is
unaffected either way — the taps keep accumulating, which is why re-enabling drains and
discards first rather than opening on a stale latched peak.

### Frontend state

`src/lib/ipc.ts` is the **only** file that talks to the backend; everything else imports
`ipc`. Outside Tauri (or with `VITE_MOCK=1`) it swaps in `src/lib/mock.ts`, a simulated
backend whose meters respond to real gain/mute/solo/routing — that's what makes
`make dev` a usable design environment. Any new IPC call must be added to the `Ipc`
interface, the Tauri impl, **and** the mock.

`mixer.svelte.ts` is optimistic: local state updates immediately, then forwards. Fader
drags throttle to one IPC call per animation frame; while a drag is active, incoming
backend state echoes are ignored for that target so the fader doesn't fight itself.
Gain commands intentionally emit no `State` event from the engine for the same reason.

`src/lib/types.ts` mirrors `lm-protocol` (serde renames to camelCase at the boundary).
**Change both together** — nothing checks this for you.

### EQ band layout

Eight EQ bands per strip, split by owner: **bands 0–3** are the user-facing EQ panel,
**bands 4–7** belong to the voice-color XY pad (voice-tuned corners at 300 Hz / 400 Hz /
3 kHz / 3.5 kHz, `EqParams::color_defaults`). The two never touch each other's indices.
`persist::sanitize` migrates older 4-band profiles up to 8.

### Persistence

TOML under `~/.config/linuxmeeter/`: `config.toml` (last profile) and
`profiles/<name>.toml`, written atomically. Saves are debounced on a 5 s engine timer
plus on shutdown. `sanitize()` clears state that must not survive a restart (solo,
`online`).

## Conventions and gotchas

- **`mockup/index.html` is the living design spec**; `src/styles/tokens.css` is the
  ported single source of truth for color/layout. Accent is amber `#FFB020`. Use the
  tokens, don't hardcode colors.
- The window is **non-resizable** and self-sizes: `StripRack.svelte` measures its groups
  and calls `setContentSize` whenever the strip/bus population changes.
- Tauri 2 window APIs silently no-op without `src-tauri/capabilities/default.json`
  granting `core:window:allow-*`. Don't remove or trim that file.
- WebKitGTK caches Vite modules across dev-server restarts and renders the UI unstyled;
  `beforeDevCommand` in `tauri.conf.json` wipes `~/.local/share/com.stacksloth.linuxmeeter/WebKitCache`
  for that reason. `make clean-cache` does it by hand.
- The Tauri dev watcher does not reliably restart after an external `cargo build` —
  restart `make app` after Rust changes when behavior looks stale.
- `lm-engine`, `pipewire`, and `libspa` are built at `opt-level = 3` even in dev profiles;
  the meter taps run per-sample and debug builds starve audio otherwise.
- Default-output takeover writes the `default.configured.audio.sink` metadata key. The
  metadata listener deliberately ignores values starting with `lm.` so the remembered
  hardware default (used for restore) survives while we are the default.
- Runtime deps that must exist on the machine: PipeWire ≥ 1.0 + WirePlumber,
  `lsp-plugins-lv2`, `webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator`.
