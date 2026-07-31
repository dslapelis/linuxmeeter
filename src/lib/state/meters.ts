/** Non-reactive meter pipeline — the performance keystone.
 *
 * Meter data NEVER touches Svelte reactive state: backend frames land in a
 * plain Map, one shared requestAnimationFrame loop advances ballistics
 * (attack/decay smoothing, peak-hold, clip latch) and calls each registered
 * canvas renderer. Backend sends ~30 fps; the loop interpolates to display
 * refresh.
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
}

const SILENT = -90;

function newMeterState(): MeterState {
  return {
    targetPeak: [SILENT, SILENT],
    targetRms: [SILENT, SILENT],
    peak: [SILENT, SILENT],
    rms: [SILENT, SILENT],
    hold: [SILENT, SILENT],
    holdAt: [0, 0],
    clip: false,
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

export function clearClip(key: MeterKey): void {
  const s = states.get(key);
  if (s) s.clip = false;
}

export function clearAllClips(): void {
  for (const s of states.values()) s.clip = false;
}

/** Called by the IPC layer for every incoming frame. */
export function ingestFrame(frame: MeterFrame, stripIds: number[], busIds: string[]): void {
  frame.strips.forEach((values, i) => {
    const id = stripIds[i];
    if (id === undefined) return;
    applyTargets(getMeter(`s:${id}`), values);
  });
  frame.buses.forEach((values, i) => {
    const id = busIds[i];
    if (id === undefined) return;
    applyTargets(getMeter(`b:${id}` as MeterKey), values);
  });
}

function applyTargets(s: MeterState, v: number[]): void {
  s.targetPeak[0] = v[0] ?? SILENT;
  s.targetPeak[1] = v[1] ?? SILENT;
  s.targetRms[0] = v[2] ?? SILENT;
  s.targetRms[1] = v[3] ?? SILENT;
}

/** Canvas components register their draw callback; one shared rAF loop runs them. */
export function registerRenderer(key: MeterKey, draw: () => void): () => void {
  renderers.set(key, draw);
  startLoop();
  return () => renderers.delete(key);
}

let running = false;
let lastTime = 0;

function startLoop(): void {
  if (running) return;
  running = true;
  lastTime = performance.now();
  requestAnimationFrame(tick);
}

function tick(now: number): void {
  if (renderers.size === 0) {
    running = false;
    return;
  }
  const dt = Math.min(0.1, (now - lastTime) / 1000);
  lastTime = now;

  for (const s of states.values()) {
    for (let ch = 0; ch < 2; ch++) {
      const tp = s.targetPeak[ch]!;
      const tr = s.targetRms[ch]!;
      // Peak: instant attack, fast fall.
      s.peak[ch] = tp > s.peak[ch]! ? tp : Math.max(SILENT, s.peak[ch]! - 40 * dt);
      // RMS: smoothed both ways.
      s.rms[ch]! += (tr - s.rms[ch]!) * Math.min(1, dt * 12);
      // Peak hold: 1.5 s, then 12 dB/s decay.
      if (s.peak[ch]! > s.hold[ch]!) {
        s.hold[ch] = s.peak[ch]!;
        s.holdAt[ch] = now;
      } else if (now - s.holdAt[ch]! > 1500) {
        s.hold[ch] = Math.max(SILENT, s.hold[ch]! - 12 * dt);
      }
      if (s.peak[ch]! >= -0.3) s.clip = true;
    }
  }

  for (const draw of renderers.values()) draw();
  requestAnimationFrame(tick);
}
