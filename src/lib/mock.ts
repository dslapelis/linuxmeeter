/** Fake backend: realistic AppState + synthesized 30 fps meter stream.
 *
 * The simulation honors live mixer state (gain, mute, solo, routing), so bus
 * meters actually respond when you pull a fader or punch a route button —
 * design iteration feels like the real app.
 */
import type { AppState, BusId, MeterFrame, StripState } from "./types";
import type { Ipc, Target } from "./ipc";
import { powerSum } from "./utils/db";

type SimKind = "speech" | "music" | "steady";

interface Sim {
  kind: SimKind;
  base: number;
  spread: number;
  phase: number;
  noise: number;
  out: number;
}

function makeSim(kind: SimKind, base: number, spread: number, phase: number): Sim {
  return { kind, base, spread, phase, noise: 0, out: -90 };
}

function simLevel(s: Sim, t: number, dt: number): number {
  s.noise = Math.max(-3, Math.min(3, (s.noise + (Math.random() - 0.5) * dt * 40) * 0.985));
  let db: number;
  if (s.kind === "speech") {
    const active = Math.sin(t * 0.55 + s.phase) + Math.sin(t * 1.7 + s.phase * 2) * 0.5 > -0.1;
    db = active ? s.base + Math.sin(t * 6 + s.phase) * s.spread * 0.4 + s.noise * 2 : -90;
  } else if (s.kind === "music") {
    const beat = Math.pow(Math.max(0, Math.sin(t * Math.PI * 2 * 1.9 + s.phase)), 10) * 7;
    db = s.base + beat + Math.sin(t * 0.23 + s.phase) * s.spread * 0.5 + s.noise;
  } else {
    db = s.base + Math.sin(t * 0.4 + s.phase) * s.spread * 0.5 + s.noise;
  }
  s.out += (db - s.out) * Math.min(1, dt * (db > s.out ? 30 : 9));
  return s.out;
}

const defaultGate = () => ({ enabled: false, thresholdDb: -40, attackMs: 20, releaseMs: 100, holdMs: 50 });
const defaultComp = () => ({ enabled: false, thresholdDb: -18, ratio: 4, attackMs: 20, releaseMs: 100, makeupDb: 0 });
const defaultLimiter = () => ({ enabled: true, thresholdDb: -1, releaseMs: 50 });

const routes = (on: BusId[]): Record<BusId, boolean> => ({
  A1: on.includes("A1"),
  A2: on.includes("A2"),
  B1: on.includes("B1"),
  B2: on.includes("B2"),
});

const state: AppState = {
  profileName: "Default",
  takeDefaultOutput: false,
  strips: [
    { id: 1, kind: "hardware", label: "Mic", hwKey: "alsa_input.usb-ZOOM_Corporation_UAC-2", online: true, gainDb: -4, mute: false, solo: false, routes: routes(["A2", "B1"]), gate: defaultGate(), comp: defaultComp() },
    { id: 2, kind: "hardware", label: "Line In", hwKey: "alsa_input.pci-0000_31_00.4.analog-stereo", online: true, gainDb: -12, mute: false, solo: false, routes: routes(["A1"]), gate: defaultGate(), comp: defaultComp() },
    { id: 3, kind: "virtual", label: "System", hwKey: null, online: true, gainDb: 0, mute: false, solo: false, routes: routes(["A1", "A2"]), gate: defaultGate(), comp: defaultComp() },
    { id: 4, kind: "virtual", label: "Music", hwKey: null, online: true, gainDb: -3.2, mute: false, solo: false, routes: routes(["A1", "A2", "B1"]), gate: defaultGate(), comp: defaultComp() },
    { id: 5, kind: "virtual", label: "Chat", hwKey: null, online: true, gainDb: -6, mute: false, solo: false, routes: routes(["A2", "B1"]), gate: defaultGate(), comp: defaultComp() },
  ],
  buses: [
    { id: "A1", label: "Speakers", targetHwKey: "alsa_output.pci-0000_2f_00.1.hdmi-stereo", online: true, gainDb: 0, mute: false, limiter: defaultLimiter() },
    { id: "A2", label: "Headphones", targetHwKey: "alsa_output.usb-ZOOM_Corporation_UAC-2", online: true, gainDb: -2.5, mute: false, limiter: defaultLimiter() },
    { id: "B1", label: "Stream Mic", targetHwKey: null, online: true, gainDb: 0, mute: false, limiter: defaultLimiter() },
    { id: "B2", label: "Aux", targetHwKey: null, online: true, gainDb: -6, mute: false, limiter: defaultLimiter() },
  ],
  devices: [
    { nodeName: "alsa_input.usb-ZOOM_Corporation_UAC-2", description: "ZOOM UAC-2", mediaClass: "Audio/Source", serial: 67, channels: 2 },
    { nodeName: "alsa_input.pci-0000_31_00.4.analog-stereo", description: "Onboard Analog", mediaClass: "Audio/Source", serial: 63, channels: 2 },
    { nodeName: "alsa_input.usb-046d_HD_Pro_Webcam_C920", description: "C920 Webcam Mic", mediaClass: "Audio/Source", serial: 45, channels: 2 },
    { nodeName: "alsa_output.pci-0000_2f_00.1.hdmi-stereo", description: "HDMI / DisplayPort", mediaClass: "Audio/Sink", serial: 65, channels: 2 },
    { nodeName: "alsa_output.usb-ZOOM_Corporation_UAC-2", description: "ZOOM UAC-2", mediaClass: "Audio/Sink", serial: 66, channels: 2 },
  ],
};

const sims = new Map<number, Sim>([
  [1, makeSim("speech", -24, 9, 1.7)],
  [2, makeSim("steady", -36, 3, 4.1)],
  [3, makeSim("steady", -30, 6, 2.3)],
  [4, makeSim("music", -11, 4, 0.4)],
  [5, makeSim("speech", -22, 8, 3.6)],
]);

function findStrip(id: number): StripState | undefined {
  return state.strips.find((s) => s.id === id);
}

let mockAutostart = false;

const meterSubs = new Set<(f: MeterFrame) => void>();
const stateSubs = new Set<(s: AppState) => void>();
let seq = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let lastTick = 0;

function startSim(): void {
  if (timer) return;
  lastTick = performance.now();
  timer = setInterval(() => {
    const now = performance.now();
    const t = now / 1000;
    const dt = Math.min(0.1, (now - lastTick) / 1000);
    lastTick = now;

    const anySolo = state.strips.some((s) => s.solo);
    const levels = new Map<number, number>();
    const stripFrames: number[][] = state.strips.map((s) => {
      const raw = simLevel(sims.get(s.id)!, t, dt);
      const audible = !s.mute && (!anySolo || s.solo) && s.online;
      const lvl = audible && s.gainDb !== -Infinity ? raw + s.gainDb : -90;
      levels.set(s.id, lvl);
      const jitter = () => Math.random() * 1.2;
      return [lvl + 0.6 + jitter(), lvl - 0.6 + jitter(), lvl - 2.5, lvl - 2.8];
    });
    const busFrames: number[][] = state.buses.map((b) => {
      const inputs = state.strips.filter((s) => s.routes[b.id]).map((s) => levels.get(s.id)!);
      const sum = b.mute ? -90 : powerSum(inputs) + b.gainDb;
      return [sum + 0.4, sum - 0.4, sum - 2.5, sum - 2.7];
    });

    const frame: MeterFrame = { seq: seq++, strips: stripFrames, buses: busFrames };
    for (const cb of meterSubs) cb(frame);
  }, 33);
}

function target(t: Target): { gainDb: number; mute: boolean } | undefined {
  return "strip" in t ? findStrip(t.strip) : state.buses.find((b) => b.id === t.bus);
}

export const mockIpc: Ipc = {
  getState: () => Promise.resolve(structuredClone(state)),
  setGain: (t, db) => {
    const o = target(t);
    if (o) o.gainDb = db;
    return Promise.resolve();
  },
  setMute: (t, mute) => {
    const o = target(t);
    if (o) o.mute = mute;
    return Promise.resolve();
  },
  setSolo: (stripId, solo) => {
    const s = findStrip(stripId);
    if (s) s.solo = solo;
    return Promise.resolve();
  },
  setRoute: (stripId, bus, on) => {
    const s = findStrip(stripId);
    if (s) s.routes[bus] = on;
    return Promise.resolve();
  },
  setGateParams: (stripId, params) => {
    const s = findStrip(stripId);
    if (s) s.gate = { ...params };
    return Promise.resolve();
  },
  setCompParams: (stripId, params) => {
    const s = findStrip(stripId);
    if (s) s.comp = { ...params };
    return Promise.resolve();
  },
  setLimiterParams: (bus, params) => {
    const b = state.buses.find((x) => x.id === bus);
    if (b) b.limiter = { ...params };
    return Promise.resolve();
  },
  setStripDevice: (stripId, hwKey) => {
    const s = findStrip(stripId);
    if (s) s.hwKey = hwKey;
    return Promise.resolve();
  },
  setBusTarget: (bus, hwKey) => {
    const b = state.buses.find((x) => x.id === bus);
    if (b) b.targetHwKey = hwKey;
    return Promise.resolve();
  },
  setDefaultOutput: (on) => {
    state.takeDefaultOutput = on;
    return Promise.resolve();
  },
  getAutostart: () => Promise.resolve(mockAutostart),
  setAutostart: (on) => {
    mockAutostart = on;
    return Promise.resolve();
  },
  onMeters: (cb) => {
    meterSubs.add(cb);
    startSim();
    return () => meterSubs.delete(cb);
  },
  onStateChanged: (cb) => {
    stateSubs.add(cb);
    return () => stateSubs.delete(cb);
  },
};
