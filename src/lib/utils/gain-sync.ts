/** Keep optimistic fader values until the engine echoes the latest write. */
export interface PendingGain { db: number; expires: number }

export function reconcileGain(incoming: number, pending: PendingGain | undefined, now: number): {
  value: number; waiting: boolean;
} {
  if (!pending) return { value: incoming, waiting: false };
  const acknowledged = incoming === pending.db || Math.abs(incoming - pending.db) < 0.0001;
  if (acknowledged || now >= pending.expires) return { value: incoming, waiting: false };
  return { value: pending.db, waiting: true };
}
