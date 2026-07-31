/** The ONLY file that talks to the backend. Everything else imports `ipc`.
 *
 * In a browser (no Tauri runtime) or with VITE_MOCK=1 the mock backend is
 * used, so `pnpm dev` gives a fully animated console for design iteration.
 */
import type { AppState, BusId, CompParams, GateParams, LimiterParams, MeterFrame } from "./types";
import { mockIpc } from "./mock";

export type Target = { strip: number } | { bus: BusId };

export interface Ipc {
  getState(): Promise<AppState>;
  setGain(target: Target, db: number): Promise<void>;
  setMute(target: Target, mute: boolean): Promise<void>;
  setSolo(stripId: number, solo: boolean): Promise<void>;
  setRoute(stripId: number, bus: BusId, on: boolean): Promise<void>;
  setGateParams(stripId: number, params: GateParams): Promise<void>;
  setCompParams(stripId: number, params: CompParams): Promise<void>;
  setLimiterParams(bus: BusId, params: LimiterParams): Promise<void>;
  setStripDevice(stripId: number, hwKey: string): Promise<void>;
  setBusTarget(bus: BusId, hwKey: string): Promise<void>;
  setDefaultOutput(on: boolean): Promise<void>;
  /** Subscribe to meter frames; returns unsubscribe. */
  onMeters(cb: (frame: MeterFrame) => void): () => void;
  /** Backend-initiated state changes (device hotplug, external volume change…). */
  onStateChanged(cb: (state: AppState) => void): () => void;
}

export const IS_TAURI = "__TAURI_INTERNALS__" in window;
export const IS_MOCK = !!import.meta.env.VITE_MOCK || !IS_TAURI;

function tauriIpc(): Ipc {
  // Dynamic import keeps @tauri-apps/api out of the browser mock path.
  const core = import("@tauri-apps/api/core");
  const event = import("@tauri-apps/api/event");
  const invoke = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> =>
    (await core).invoke<T>(cmd, args);

  return {
    getState: () => invoke("get_state"),
    setGain: (target, db) => invoke("set_gain", { target, db }),
    setMute: (target, mute) => invoke("set_mute", { target, mute }),
    setSolo: (stripId, solo) => invoke("set_solo", { stripId, solo }),
    setRoute: (stripId, bus, on) => invoke("set_route", { stripId, bus, on }),
    setGateParams: (stripId, params) => invoke("set_gate_params", { stripId, params }),
    setCompParams: (stripId, params) => invoke("set_comp_params", { stripId, params }),
    setLimiterParams: (bus, params) => invoke("set_limiter_params", { bus, params }),
    setStripDevice: (stripId, hwKey) => invoke("set_strip_device", { stripId, hwKey }),
    setBusTarget: (bus, hwKey) => invoke("set_bus_target", { bus, hwKey }),
    setDefaultOutput: (on) => invoke("set_default_output", { on }),
    onMeters: (cb) => {
      const un = event.then((e) => e.listen<MeterFrame>("meters", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
    onStateChanged: (cb) => {
      const un = event.then((e) => e.listen<AppState>("state_changed", (ev) => cb(ev.payload)));
      return () => void un.then((f) => f());
    },
  };
}

export const ipc: Ipc = IS_MOCK ? mockIpc : tauriIpc();
