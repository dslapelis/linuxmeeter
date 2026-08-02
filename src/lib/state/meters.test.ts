import { beforeEach, describe, expect, it } from "vitest";
import { advance, clearAllClips, clearClip, getMeter, ingestFrame } from "./meters";
import type { MeterFrame, MeterKey } from "../types";

const frame = (strips: number[][], buses: number[][] = []): MeterFrame => ({
  seq: 1,
  strips,
  buses,
});

describe("meter frame ingest", () => {
  beforeEach(() => clearAllClips());

  /// Frames are positional: entry i belongs to strips[i]. Getting this wrong
  /// shows every strip's audio on the wrong meter.
  it("maps frame entries onto ids by position", () => {
    ingestFrame(frame([[-6, -7, -9, -10], [-20, -21, -23, -24]]), [4, 9], []);

    const first = getMeter("s:4");
    expect(first.targetPeak).toEqual([-6, -7]);
    expect(first.targetRms).toEqual([-9, -10]);

    const second = getMeter("s:9");
    expect(second.targetPeak).toEqual([-20, -21]);
  });

  it("keys buses separately from strips", () => {
    ingestFrame(frame([[-6, -6, -9, -9]], [[-3, -3, -6, -6]]), [1], ["A1"]);
    expect(getMeter("s:1").targetPeak[0]).toBe(-6);
    expect(getMeter("b:A1").targetPeak[0]).toBe(-3);
  });

  /// A frame can arrive in the window between the engine changing the strip
  /// list and the frontend applying the new state.
  it("ignores entries with no matching id", () => {
    expect(() => ingestFrame(frame([[-6, -6, -9, -9], [-1, -1, -1, -1]]), [7], [])).not.toThrow();
    expect(getMeter("s:7").targetPeak[0]).toBe(-6);
  });

  it("tolerates short channel arrays", () => {
    ingestFrame(frame([[-6]]), [3], []);
    const m = getMeter("s:3");
    expect(m.targetPeak[0]).toBe(-6);
    expect(m.targetPeak[1]).toBe(-90);
    expect(m.targetRms).toEqual([-90, -90]);
  });

  it("starts new meters at the silence floor", () => {
    const fresh = getMeter("s:12345");
    expect(fresh.targetPeak).toEqual([-90, -90]);
    expect(fresh.peak).toEqual([-90, -90]);
    expect(fresh.clip).toBe(false);
  });

  it("returns the same state object for a key so renderers stay attached", () => {
    expect(getMeter("s:1")).toBe(getMeter("s:1"));
  });
});

/// The render loop parks itself when nothing is moving, so "did anything
/// move?" is what decides whether the app burns a GPU repaint per meter per
/// frame or sits at zero. These pin that decision.
describe("ballistics settle", () => {
  let n = 0;
  const fresh = (): ReturnType<typeof getMeter> => getMeter(`s:${900000 + n++}` as MeterKey);

  /// The idle case: a silent meter that has already settled must report no
  /// movement, forever. If this regresses the loop never stops.
  it("reports no movement once a silent meter has settled", () => {
    const s = fresh();
    for (let i = 0; i < 500; i++) advance(s, 1 / 60, 1000 + i * 16.7);
    s.dirty = false; // stand in for the loop having drawn it
    expect(advance(s, 1 / 60, 100000)).toBe(false);
    expect(s.dirty).toBe(false);
  });

  it("reports movement while converging on a new target", () => {
    const s = fresh();
    s.targetPeak = [-6, -6];
    s.targetRms = [-12, -12];
    expect(advance(s, 1 / 60, 1000)).toBe(true);
    expect(s.dirty).toBe(true);
  });

  /// A steady tone holds its target constant. Once the display reaches it,
  /// redrawing the same pixels every frame is pure waste.
  it("settles on a steady signal instead of animating forever", () => {
    const s = fresh();
    s.targetPeak = [-6, -6];
    s.targetRms = [-12, -12];
    let t = 1000;
    for (let i = 0; i < 600; i++, t += 16.7) advance(s, 1 / 60, t);
    expect(advance(s, 1 / 60, t)).toBe(false);
    expect(s.rms[0]).toBeCloseTo(-12, 5);
    expect(s.peak[0]).toBeCloseTo(-6, 5);
  });

  /// Sub-pixel jitter along the noise floor must not wake the loop, or a
  /// silent mixer redraws at full rate anyway.
  it("absorbs jitter below the sub-pixel threshold", () => {
    const s = fresh();
    s.targetPeak = [-6, -6];
    s.targetRms = [-12, -12];
    let t = 1000;
    for (let i = 0; i < 600; i++, t += 16.7) advance(s, 1 / 60, t);
    s.targetRms = [-12.02, -12.02];
    expect(advance(s, 1 / 60, t)).toBe(false);
  });

  /// Peak-hold decays for a second and a half after the peak, so the loop has
  /// to keep running through it.
  it("keeps moving while the peak hold decays", () => {
    const s = fresh();
    s.targetPeak = [-6, -6];
    advance(s, 1 / 60, 1000);
    expect(s.hold[0]).toBeCloseTo(-6, 5);
    s.targetPeak = [-90, -90];
    // Past the 1.5 s hold window, the hold line is falling: still moving.
    expect(advance(s, 1 / 60, 3000)).toBe(true);
  });

  it("latches clip and reports it as movement exactly once", () => {
    const s = fresh();
    s.targetPeak = [0, 0];
    advance(s, 1 / 60, 1000);
    expect(s.clip).toBe(true);
    s.dirty = false;
    // Still clipping, but the latch is already set — nothing new to draw.
    const moved = advance(s, 1 / 60, 1016);
    expect(s.clip).toBe(true);
    expect(moved).toBe(false);
  });
});

describe("clip latch", () => {
  it("clears one meter without touching the others", () => {
    const a = getMeter("s:100");
    const b = getMeter("s:101");
    a.clip = true;
    b.clip = true;

    clearClip("s:100");
    expect(a.clip).toBe(false);
    expect(b.clip).toBe(true);

    clearAllClips();
    expect(b.clip).toBe(false);
  });

  it("ignores an unknown key", () => {
    expect(() => clearClip("s:999999")).not.toThrow();
  });
});
