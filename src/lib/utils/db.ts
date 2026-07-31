/** Fader taper, dB formatting, meter zone math. */

/** Piecewise-linear audio taper: ~55% of fader travel covers −12..+12 dB. */
const BREAKPOINTS: Array<[number, number]> = [
  [0, -72],
  [0.08, -50],
  [0.3, -30],
  [0.55, -12],
  [0.78, 0],
  [1, 12],
];

export const FADER_MIN_DB = -72; // rendered as −∞
export const FADER_MAX_DB = 12;

export function posToDb(p: number): number {
  if (p <= 0) return -Infinity;
  p = Math.min(1, p);
  for (let i = 0; i < BREAKPOINTS.length - 1; i++) {
    const [p0, d0] = BREAKPOINTS[i]!;
    const [p1, d1] = BREAKPOINTS[i + 1]!;
    if (p <= p1) return d0 + ((d1 - d0) * (p - p0)) / (p1 - p0);
  }
  return FADER_MAX_DB;
}

export function dbToPos(db: number): number {
  if (db === -Infinity || db <= FADER_MIN_DB) return 0;
  db = Math.min(FADER_MAX_DB, db);
  for (let i = 0; i < BREAKPOINTS.length - 1; i++) {
    const [p0, d0] = BREAKPOINTS[i]!;
    const [p1, d1] = BREAKPOINTS[i + 1]!;
    if (db <= d1) return p0 + ((p1 - p0) * (db - d0)) / (d1 - d0);
  }
  return 1;
}

export function fmtDb(db: number): string {
  if (db === -Infinity || db <= FADER_MIN_DB + 1) return "-∞";
  return (db > 0 ? "+" : "") + db.toFixed(1);
}

export function dbToLinear(db: number): number {
  return Math.pow(10, db / 20);
}

export function linearToDb(linear: number): number {
  return linear <= 0 ? -Infinity : 20 * Math.log10(linear);
}

/** dB power sum (for mock bus metering). */
export function powerSum(dbs: number[]): number {
  let p = 0;
  for (const db of dbs) if (db > -85) p += Math.pow(10, db / 10);
  return p <= 0 ? -90 : 10 * Math.log10(p);
}
