import { describe, expect, it } from "vitest";
import { EQ_FMAX, EQ_FMIN, freqToX, responseCurve, xToFreq } from "./eq";
import type { EqBand } from "../types";

const band = (over: Partial<EqBand> = {}): EqBand => ({
  kind: "peak",
  freqHz: 1000,
  gainDb: 0,
  q: 1,
  ...over,
});

/** dB of the summed response at the frequency closest to `f`. */
function dbAt(bands: EqBand[], f: number, points = 512): number {
  const curve = responseCurve(bands, points);
  let best = curve[0]!;
  for (const p of curve) {
    if (Math.abs(Math.log(p.f / f)) < Math.abs(Math.log(best.f / f))) best = p;
  }
  return best.db;
}

describe("frequency axis", () => {
  it("maps the audible range onto the full width", () => {
    expect(freqToX(EQ_FMIN, 100)).toBeCloseTo(0, 9);
    expect(freqToX(EQ_FMAX, 100)).toBeCloseTo(100, 9);
  });

  it("is logarithmic — each decade takes equal width", () => {
    const decade1 = freqToX(200, 900) - freqToX(20, 900);
    const decade2 = freqToX(2000, 900) - freqToX(200, 900);
    expect(decade2).toBeCloseTo(decade1, 6);
  });

  it("round-trips position and frequency", () => {
    for (const f of [20, 100, 440, 1000, 5000, 20000]) {
      expect(xToFreq(freqToX(f, 640), 640)).toBeCloseTo(f, 3);
    }
  });
});

describe("EQ response curve", () => {
  it("is flat when every band is at 0 dB", () => {
    const curve = responseCurve([band(), band({ freqHz: 100 }), band({ freqHz: 8000 })], 128);
    for (const p of curve) expect(Math.abs(p.db)).toBeLessThan(0.01);
  });

  it("spans the audible range and returns the requested resolution", () => {
    const curve = responseCurve([band()], 200);
    expect(curve).toHaveLength(200);
    expect(curve[0]!.f).toBeCloseTo(EQ_FMIN, 6);
    expect(curve[199]!.f).toBeCloseTo(EQ_FMAX, 6);
  });

  it("puts a peak band's boost at its centre frequency", () => {
    const bands = [band({ freqHz: 1000, gainDb: 6, q: 2 })];
    expect(dbAt(bands, 1000)).toBeCloseTo(6, 1);
    // and leaves distant frequencies alone
    expect(Math.abs(dbAt(bands, 60))).toBeLessThan(0.5);
    expect(Math.abs(dbAt(bands, 15000))).toBeLessThan(0.5);
  });

  it("cuts as well as boosts", () => {
    expect(dbAt([band({ freqHz: 1000, gainDb: -9, q: 2 })], 1000)).toBeCloseTo(-9, 1);
  });

  it("makes a low shelf lift the bottom and leave the top", () => {
    const bands = [band({ kind: "low_shelf", freqHz: 200, gainDb: 6, q: 0.7 })];
    expect(dbAt(bands, 25)).toBeCloseTo(6, 0);
    expect(Math.abs(dbAt(bands, 15000))).toBeLessThan(0.5);
  });

  it("makes a high shelf lift the top and leave the bottom", () => {
    const bands = [band({ kind: "high_shelf", freqHz: 4000, gainDb: 6, q: 0.7 })];
    expect(dbAt(bands, 18000)).toBeCloseTo(6, 0);
    expect(Math.abs(dbAt(bands, 30))).toBeLessThan(0.5);
  });

  /// The displayed curve is the sum of the bands, which is what makes the EQ
  /// panel's overlay meaningful.
  it("sums overlapping bands", () => {
    const two = [band({ freqHz: 1000, gainDb: 4, q: 1 }), band({ freqHz: 1000, gainDb: 3, q: 1 })];
    expect(dbAt(two, 1000)).toBeCloseTo(7, 1);
  });

  it("narrows the affected range as Q rises", () => {
    const wide = [band({ freqHz: 1000, gainDb: 12, q: 0.5 })];
    const narrow = [band({ freqHz: 1000, gainDb: 12, q: 8 })];
    expect(dbAt(wide, 1000)).toBeCloseTo(12, 0);
    expect(dbAt(narrow, 1000)).toBeCloseTo(12, 0);
    // An octave away the wide band still does a lot; the narrow one barely.
    expect(dbAt(wide, 2000)).toBeGreaterThan(dbAt(narrow, 2000) + 3);
  });

  it("survives extreme frequencies without producing NaN", () => {
    for (const f of [10, 19, 20000, 24000]) {
      const curve = responseCurve([band({ freqHz: f, gainDb: 6 })], 64);
      for (const p of curve) expect(Number.isFinite(p.db)).toBe(true);
    }
  });
});
