/** Reactive control state + optimistic mutations.
 *
 * User interaction writes local state immediately, then forwards to the
 * backend. Continuous controls (fader drags) throttle IPC to one call per
 * animation frame, with a guaranteed final send.
 */
import type { AppState, BusId, BusState, CompParams, DeviceInfo, GateParams, LimiterParams, StripState } from "../types";
import { ipc, type Target } from "../ipc";
import { ingestFrame } from "./meters";

class MixerStore {
  strips = $state<StripState[]>([]);
  buses = $state<BusState[]>([]);
  devices = $state<DeviceInfo[]>([]);
  profileName = $state("Default");
  takeDefaultOutput = $state(false);
  ready = $state(false);
  error = $state<string | null>(null);

  anySolo = $derived(this.strips.some((s) => s.solo));

  /** Per-target pending gain send (rAF-throttled). */
  #pendingGain = new Map<string, { target: Target; db: number }>();
  #flushScheduled = false;
  /** Targets with an active drag: incoming backend echoes are ignored for these. */
  #dragging = new Set<string>();

  async init(): Promise<void> {
    // The engine builds its PipeWire graph asynchronously at startup; the
    // backend command also waits, so one retry covers slow cold starts.
    let s: AppState;
    try {
      s = await ipc.getState();
    } catch {
      await new Promise((r) => setTimeout(r, 1000));
      s = await ipc.getState();
    }
    this.#apply(s);
    this.ready = true;
    this.error = null;
    ipc.onMeters((frame) =>
      ingestFrame(
        frame,
        this.strips.map((x) => x.id),
        this.buses.map((x) => x.id),
      ),
    );
    ipc.onStateChanged((state) => this.#apply(state, true));
  }

  #apply(s: AppState, guardDragging = false): void {
    if (guardDragging && this.#dragging.size > 0) {
      // Keep locally-dragged gains; take everything else.
      const held = new Map<string, number>();
      for (const key of this.#dragging) {
        const t = keyToTarget(key);
        const cur = this.#find(t);
        if (cur) held.set(key, cur.gainDb);
      }
      this.strips = s.strips;
      this.buses = s.buses;
      for (const [key, db] of held) {
        const o = this.#find(keyToTarget(key));
        if (o) o.gainDb = db;
      }
    } else {
      this.strips = s.strips;
      this.buses = s.buses;
    }
    this.devices = s.devices;
    this.profileName = s.profileName;
    this.takeDefaultOutput = s.takeDefaultOutput;
  }

  #find(t: Target): StripState | BusState | undefined {
    return "strip" in t ? this.strips.find((s) => s.id === t.strip) : this.buses.find((b) => b.id === t.bus);
  }

  strip(id: number): StripState | undefined {
    return this.strips.find((s) => s.id === id);
  }

  bus(id: BusId): BusState | undefined {
    return this.buses.find((b) => b.id === id);
  }

  beginGainDrag(target: Target): void {
    this.#dragging.add(targetKey(target));
  }

  endGainDrag(target: Target): void {
    this.#dragging.delete(targetKey(target));
    this.#flushGains();
  }

  setGain(target: Target, db: number): void {
    const o = this.#find(target);
    if (o) o.gainDb = db;
    this.#pendingGain.set(targetKey(target), { target, db });
    if (!this.#flushScheduled) {
      this.#flushScheduled = true;
      requestAnimationFrame(() => this.#flushGains());
    }
  }

  #flushGains(): void {
    this.#flushScheduled = false;
    for (const { target, db } of this.#pendingGain.values()) void ipc.setGain(target, db);
    this.#pendingGain.clear();
  }

  setMute(target: Target, mute: boolean): void {
    const o = this.#find(target);
    if (o) o.mute = mute;
    void ipc.setMute(target, mute);
  }

  setSolo(stripId: number, solo: boolean): void {
    const s = this.strip(stripId);
    if (s) s.solo = solo;
    void ipc.setSolo(stripId, solo);
  }

  setRoute(stripId: number, bus: BusId, on: boolean): void {
    const s = this.strip(stripId);
    if (s) s.routes[bus] = on;
    void ipc.setRoute(stripId, bus, on);
  }

  setGateParams(stripId: number, params: GateParams): void {
    const s = this.strip(stripId);
    if (s) s.gate = params;
    void ipc.setGateParams(stripId, params);
  }

  setCompParams(stripId: number, params: CompParams): void {
    const s = this.strip(stripId);
    if (s) s.comp = params;
    void ipc.setCompParams(stripId, params);
  }

  setLimiterParams(bus: BusId, params: LimiterParams): void {
    const b = this.bus(bus);
    if (b) b.limiter = params;
    void ipc.setLimiterParams(bus, params);
  }

  setStripDevice(stripId: number, hwKey: string): void {
    const s = this.strip(stripId);
    if (s) s.hwKey = hwKey;
    void ipc.setStripDevice(stripId, hwKey);
  }

  setBusTarget(bus: BusId, hwKey: string): void {
    const b = this.bus(bus);
    if (b) b.targetHwKey = hwKey;
    void ipc.setBusTarget(bus, hwKey);
  }

  setDefaultOutput(on: boolean): void {
    this.takeDefaultOutput = on;
    void ipc.setDefaultOutput(on);
  }
}

function targetKey(t: Target): string {
  return "strip" in t ? `s:${t.strip}` : `b:${t.bus}`;
}

function keyToTarget(key: string): Target {
  const [kind, id] = key.split(":");
  return kind === "s" ? { strip: Number(id) } : { bus: id as BusId };
}

export const mixer = new MixerStore();
