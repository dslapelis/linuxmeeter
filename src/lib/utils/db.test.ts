import { describe, expect, it } from "vitest";
import {
  FADER_MAX_DB,
  FADER_MIN_DB,
  dbToLinear,
  dbToPos,
  fmtDb,
  linearToDb,
  posToDb,
  powerSum,
} from "./db";

describe("fader taper", () => {
  it("bottoms out at silence and tops out at +12", () => {
    expect(posToDb(0)).toBe(-Infinity);
    expect(posToDb(1)).toBeCloseTo(FADER_MAX_DB, 6);
    expect(dbToPos(-Infinity)).toBe(0);
    expect(dbToPos(FADER_MAX_DB)).toBeCloseTo(1, 6);
  });

  it("round-trips position through dB", () => {
    for (let p = 0.02; p <= 1; p += 0.02) {
      expect(dbToPos(posToDb(p))).toBeCloseTo(p, 5);
    }
  });

  it("round-trips dB through position", () => {
    for (let db = FADER_MIN_DB + 1; db <= FADER_MAX_DB; db += 1.5) {
      expect(posToDb(dbToPos(db))).toBeCloseTo(db, 4);
    }
  });

  /// A fader that is not monotonic jumps around under the cursor.
  it("is strictly increasing", () => {
    let prev = -Infinity;
    for (let p = 0.01; p <= 1; p += 0.01) {
      const db = posToDb(p);
      expect(db).toBeGreaterThan(prev);
      prev = db;
    }
  });

  /// Unity sits high on the throw so the useful range gets the most travel —
  /// this is the whole point of a piecewise taper over a linear one.
  it("puts unity gain near three-quarters of the throw", () => {
    expect(dbToPos(0)).toBeCloseTo(0.78, 2);
    expect(posToDb(0.78)).toBeCloseTo(0, 4);
  });

  it("gives the -12..+12 dB region a large share of the travel", () => {
    const share = dbToPos(12) - dbToPos(-12);
    expect(share).toBeGreaterThan(0.4);
  });

  it("clamps beyond the ends instead of extrapolating", () => {
    expect(posToDb(2)).toBe(FADER_MAX_DB);
    expect(posToDb(-1)).toBe(-Infinity);
    expect(dbToPos(99)).toBe(1);
    expect(dbToPos(-999)).toBe(0);
  });
});

describe("dB formatting", () => {
  it("shows silence as -∞", () => {
    expect(fmtDb(-Infinity)).toBe("-∞");
    expect(fmtDb(FADER_MIN_DB)).toBe("-∞");
  });

  it("signs positive values and fixes one decimal", () => {
    expect(fmtDb(0)).toBe("0.0");
    expect(fmtDb(3)).toBe("+3.0");
    expect(fmtDb(-6.25)).toBe("-6.3");
  });
});

describe("gain conversion", () => {
  /// Must agree with `params::db_to_linear` in lm-engine — the frontend and
  /// the DSP have to mean the same thing by "-6 dB".
  it("matches the standard 20*log10 relationship", () => {
    expect(dbToLinear(0)).toBeCloseTo(1, 9);
    expect(dbToLinear(-20)).toBeCloseTo(0.1, 9);
    expect(dbToLinear(-6)).toBeCloseTo(0.5011872, 6);
    expect(linearToDb(1)).toBeCloseTo(0, 9);
    expect(linearToDb(0.5)).toBeCloseTo(-6.0206, 3);
  });

  it("round-trips", () => {
    for (const db of [-60, -18, -6, 0, 6, 12]) {
      expect(linearToDb(dbToLinear(db))).toBeCloseTo(db, 6);
    }
  });

  it("treats zero and negative gain as silence", () => {
    expect(linearToDb(0)).toBe(-Infinity);
    expect(linearToDb(-1)).toBe(-Infinity);
  });
});

describe("power sum", () => {
  /// Two equal sources are 3 dB louder than one — the reason bus meters use a
  /// power sum rather than picking a maximum.
  it("adds two equal signals to +3 dB", () => {
    expect(powerSum([-6, -6])).toBeCloseTo(-6 + 3.0103, 3);
  });

  it("is dominated by the loudest source", () => {
    expect(powerSum([-6, -40])).toBeCloseTo(-6, 1);
  });

  it("returns the floor for nothing at all", () => {
    expect(powerSum([])).toBe(-90);
    expect(powerSum([-90, -95])).toBe(-90);
  });

  it("does not change a single signal", () => {
    expect(powerSum([-12])).toBeCloseTo(-12, 6);
  });
});
