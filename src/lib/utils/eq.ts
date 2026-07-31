/** EQ frequency-response math (RBJ audio-EQ-cookbook biquads).
 *
 * Approximates the LSP para_equalizer curve closely enough for display; the
 * actual filtering happens in the plugin.
 */
import type { EqBand } from "../types";

const FS = 48000;

interface Biquad {
  b0: number;
  b1: number;
  b2: number;
  a0: number;
  a1: number;
  a2: number;
}

function coeffs(band: EqBand): Biquad {
  const A = Math.pow(10, band.gainDb / 40);
  const w0 = (2 * Math.PI * Math.min(band.freqHz, FS / 2 - 1)) / FS;
  const cos = Math.cos(w0);
  const sin = Math.sin(w0);
  const q = Math.max(0.1, band.q);
  const alpha = sin / (2 * q);
  const sqA = Math.sqrt(A);

  switch (band.kind) {
    case "peak":
      return {
        b0: 1 + alpha * A,
        b1: -2 * cos,
        b2: 1 - alpha * A,
        a0: 1 + alpha / A,
        a1: -2 * cos,
        a2: 1 - alpha / A,
      };
    case "low_shelf":
      return {
        b0: A * (A + 1 - (A - 1) * cos + 2 * sqA * alpha),
        b1: 2 * A * (A - 1 - (A + 1) * cos),
        b2: A * (A + 1 - (A - 1) * cos - 2 * sqA * alpha),
        a0: A + 1 + (A - 1) * cos + 2 * sqA * alpha,
        a1: -2 * (A - 1 + (A + 1) * cos),
        a2: A + 1 + (A - 1) * cos - 2 * sqA * alpha,
      };
    case "high_shelf":
      return {
        b0: A * (A + 1 + (A - 1) * cos + 2 * sqA * alpha),
        b1: -2 * A * (A - 1 + (A + 1) * cos),
        b2: A * (A + 1 + (A - 1) * cos - 2 * sqA * alpha),
        a0: A + 1 - (A - 1) * cos + 2 * sqA * alpha,
        a1: 2 * (A - 1 - (A + 1) * cos),
        a2: A + 1 - (A - 1) * cos - 2 * sqA * alpha,
      };
  }
}

/** |H(e^jw)| in dB for one biquad at frequency f. */
function magDb(c: Biquad, f: number): number {
  const w = (2 * Math.PI * f) / FS;
  const cw = Math.cos(w);
  const c2w = Math.cos(2 * w);
  const sw = Math.sin(w);
  const s2w = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * cw + c.b2 * c2w;
  const ni = -(c.b1 * sw + c.b2 * s2w);
  const dr = c.a0 + c.a1 * cw + c.a2 * c2w;
  const di = -(c.a1 * sw + c.a2 * s2w);
  const mag = Math.sqrt((nr * nr + ni * ni) / (dr * dr + di * di));
  return 20 * Math.log10(Math.max(1e-6, mag));
}

export const EQ_FMIN = 20;
export const EQ_FMAX = 20000;

export function freqToX(f: number, width: number): number {
  return (Math.log10(f / EQ_FMIN) / Math.log10(EQ_FMAX / EQ_FMIN)) * width;
}

export function xToFreq(x: number, width: number): number {
  return EQ_FMIN * Math.pow(EQ_FMAX / EQ_FMIN, x / width);
}

/** Summed response of all bands (gain 0 bands contribute ~nothing). */
export function responseCurve(bands: EqBand[], points: number): Array<{ f: number; db: number }> {
  const cs = bands.map(coeffs);
  const out: Array<{ f: number; db: number }> = [];
  for (let i = 0; i < points; i++) {
    const f = EQ_FMIN * Math.pow(EQ_FMAX / EQ_FMIN, i / (points - 1));
    let db = 0;
    for (const c of cs) db += magDb(c, f);
    out.push({ f, db });
  }
  return out;
}
