/** Non-reactive meter pipeline — the performance keystone.
 *
 * Meter data NEVER touches Svelte reactive state: backend frames land in a
 * plain Map, one shared requestAnimationFrame loop advances ballistics
 * (attack/decay smoothing, peak-hold, clip latch) and calls each registered
 * canvas renderer. Backend sends ~30 fps; the loop interpolates to display
 * refresh.
 *
 * The loop is idle-driven, because a mixer sitting in the tray is the normal
 * case and each canvas repaint is a full Skia GPU pass:
 *
 *   - it never runs faster than MAX_FPS, however fast the display refreshes;
 *   - it redraws only the meters whose displayed values actually moved;
 *   - once nothing is moving it stops entirely, and `ingestFrame` restarts it
 *     when a target changes;
 *   - it does not run while the document is hidden.
 *
 * Changes below EPSILON_DB cannot move a meter by a whole pixel, so they are
 * absorbed rather than drawn — that is what lets a silent or steady signal
 * settle to zero redraws instead of jittering along the noise floor.
 */
import type { MeterFrame, MeterKey } from "../types";

export interface MeterState {
  /** Latest backend targets, dBFS. */
  targetPeak: [number, number];
  targetRms: [number, number];
  /** Display values after ballistics. */
  peak: [number, number];
  rms: [number, number];
  hold: [number, number];
  holdAt: [number, number];
  clip: boolean;
  /** Display values moved since this meter was last drawn. */
  dirty: boolean;
}

const SILENT = -90;

/** Displayed meters span 60 dB over ~150 px, so 0.1 dB is comfortably
 *  sub-pixel: below this a change is invisible and not worth a repaint. */
const EPSILON_DB = 0.1;

/** The engine drains meters at 30 Hz, so drawing faster than that only
 *  interpolates between frames that already arrived — it invents no new
 *  information and costs a Skia pass per canvas per frame. Matching the
 *  backend rate measured ~2x cheaper than 60 fps with signal on every strip. */
const MAX_FPS = 30;
const MIN_FRAME_MS = 1000 / MAX_FPS - 1;

function newMeterState(): MeterState {
  return {
    targetPeak: [SILENT, SILENT],
    targetRms: [SILENT, SILENT],
    peak: [SILENT, SILENT],
    rms: [SILENT, SILENT],
    hold: [SILENT, SILENT],
    holdAt: [0, 0],
    clip: false,
    // A fresh canvas is blank, so the first tick must paint it.
    dirty: true,
  };
}

const states = new Map<MeterKey, MeterState>();
const renderers = new Map<MeterKey, () => void>();

export function getMeter(key: MeterKey): MeterState {
  let s = states.get(key);
  if (!s) {
    s = newMeterState();
    states.set(key, s);
  }
  return s;
}

/** Force a repaint of one meter — used when its canvas backing store is
 *  resized, which clears it. */
export function markDirty(key: MeterKey): void {
  const s = states.get(key);
  if (s) s.dirty = true;
  startLoop();
}

export function clearClip(key: MeterKey): void {
  const s = states.get(key);
  if (s && s.clip) {
    s.clip = false;
    s.dirty = true;
    startLoop();
  }
}

export function clearAllClips(): void {
  for (const s of states.values()) {
    if (s.clip) {
      s.clip = false;
      s.dirty = true;
    }
  }
  startLoop();
}

/** Called by the IPC layer for every incoming frame. */
export function ingestFrame(frame: MeterFrame, stripIds: number[], busIds: string[]): void {
  let moved = false;
  frame.strips.forEach((values, i) => {
    const id = stripIds[i];
    if (id === undefined) return;
    if (applyTargets(getMeter(`s:${id}`), values)) moved = true;
  });
  frame.buses.forEach((values, i) => {
    const id = busIds[i];
    if (id === undefined) return;
    if (applyTargets(getMeter(`b:${id}` as MeterKey), values)) moved = true;
  });
  // Targets that did not move cannot make the display move, so a steady or
  // silent signal leaves a stopped loop stopped.
  if (moved) startLoop();
}

/** Returns whether any target moved enough to be worth animating towards. */
function applyTargets(s: MeterState, v: number[]): boolean {
  const next: [number, number, number, number] = [
    v[0] ?? SILENT,
    v[1] ?? SILENT,
    v[2] ?? SILENT,
    v[3] ?? SILENT,
  ];
  const moved =
    Math.abs(next[0] - s.targetPeak[0]) >= EPSILON_DB ||
    Math.abs(next[1] - s.targetPeak[1]) >= EPSILON_DB ||
    Math.abs(next[2] - s.targetRms[0]) >= EPSILON_DB ||
    Math.abs(next[3] - s.targetRms[1]) >= EPSILON_DB;
  s.targetPeak[0] = next[0];
  s.targetPeak[1] = next[1];
  s.targetRms[0] = next[2];
  s.targetRms[1] = next[3];
  return moved;
}

/** Canvas components register their draw callback; one shared rAF loop runs them. */
export function registerRenderer(key: MeterKey, draw: () => void): () => void {
  renderers.set(key, draw);
  getMeter(key).dirty = true;
  startLoop();
  return () => renderers.delete(key);
}

let running = false;
let lastTime = 0;

function startLoop(): void {
  if (running || renderers.size === 0) return;
  if (typeof document !== "undefined" && document.hidden) return;
  running = true;
  lastTime = performance.now();
  requestAnimationFrame(tick);
}

/** Advance one meter's ballistics; returns whether anything visible moved. */
export function advance(s: MeterState, dt: number, now: number): boolean {
  let moved = false;
  for (let ch = 0; ch < 2; ch++) {
    const tp = s.targetPeak[ch]!;
    const tr = s.targetRms[ch]!;

    // Peak: instant attack, fast fall. The comparison is `>=`, not `>`: a
    // steady tone holds its target exactly, where `>` would decay one frame
    // and re-attack the next, flickering by 40 * dt forever and never letting
    // the loop settle.
    const peak = tp >= s.peak[ch]! ? tp : Math.max(SILENT, s.peak[ch]! - 40 * dt);
    if (Math.abs(peak - s.peak[ch]!) >= EPSILON_DB) moved = true;
    s.peak[ch] = peak;

    // RMS: smoothed both ways. The approach is asymptotic, so snap once the
    // remaining gap is sub-pixel rather than crawling towards it forever.
    if (Math.abs(tr - s.rms[ch]!) < EPSILON_DB) {
      s.rms[ch] = tr;
    } else {
      s.rms[ch]! += (tr - s.rms[ch]!) * Math.min(1, dt * 12);
      moved = true;
    }

    // Peak hold: 1.5 s, then 12 dB/s decay. `>=` for the same reason as
    // above — the hold line can never sit below the peak it is holding.
    if (s.peak[ch]! >= s.hold[ch]!) {
      if (s.peak[ch]! - s.hold[ch]! >= EPSILON_DB) moved = true;
      s.hold[ch] = s.peak[ch]!;
      s.holdAt[ch] = now;
    } else if (now - s.holdAt[ch]! > 1500) {
      const hold = Math.max(SILENT, s.hold[ch]! - 12 * dt);
      if (Math.abs(hold - s.hold[ch]!) >= EPSILON_DB) moved = true;
      s.hold[ch] = hold;
    }

    if (s.peak[ch]! >= -0.3 && !s.clip) {
      s.clip = true;
      moved = true;
    }
  }
  if (moved) s.dirty = true;
  return moved;
}

function tick(now: number): void {
  if (renderers.size === 0 || (typeof document !== "undefined" && document.hidden)) {
    running = false;
    return;
  }
  // Cap the rate without consuming the elapsed time, so the next accepted
  // frame still advances the ballistics by the full interval.
  if (now - lastTime < MIN_FRAME_MS) {
    requestAnimationFrame(tick);
    return;
  }
  const dt = Math.min(0.1, (now - lastTime) / 1000);
  lastTime = now;

  for (const s of states.values()) advance(s, dt, now);

  let drew = false;
  for (const [key, draw] of renderers) {
    const s = states.get(key);
    if (s?.dirty) {
      draw();
      s.dirty = false;
      drew = true;
    }
  }

  // Nothing moved and nothing was pending: park until a frame or an
  // interaction wakes us. This is the tray/idle case, and it is the norm.
  if (!drew) {
    running = false;
    return;
  }
  requestAnimationFrame(tick);
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) return;
    // Coming back from the tray, repaint unconditionally: WebKit may have
    // dropped the canvas backing stores while the view was unmapped, and a
    // silent or steady mixer would otherwise never move enough to redraw them.
    for (const s of states.values()) s.dirty = true;
    startLoop();
  });
}
