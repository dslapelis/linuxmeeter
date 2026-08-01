import { beforeEach, describe, expect, it } from "vitest";
import { clearAllClips, clearClip, getMeter, ingestFrame } from "./meters";
import type { MeterFrame } from "../types";

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
