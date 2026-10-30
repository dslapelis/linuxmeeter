import { describe, expect, it } from 'vitest';
import { reconcileGain } from './gain-sync';

describe('fader state echoes', () => {
  it('does not roll a newer wheel change back to an older engine echo', () => {
    expect(reconcileGain(-12, { db: -10, expires: 2000 }, 100)).toEqual({ value: -10, waiting: true });
  });
  it('accepts acknowledgement with float32 rounding', () => {
    expect(reconcileGain(-10.123456, { db: -10.1234557, expires: 2000 }, 100).waiting).toBe(false);
  });
  it('accepts later external changes once a local write is acknowledged', () => {
    expect(reconcileGain(-24, undefined, 200)).toEqual({ value: -24, waiting: false });
  });
  it('expires an unacknowledged local write instead of pinning the UI forever', () => {
    expect(reconcileGain(-24, { db: -10, expires: 100 }, 101)).toEqual({ value: -24, waiting: false });
  });
  it('handles the silence sentinel', () => {
    expect(reconcileGain(-Infinity, { db: -Infinity, expires: 2000 }, 100).waiting).toBe(false);
  });
});
