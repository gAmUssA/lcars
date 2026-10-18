// Decides which transitions deserve a ping, given consecutive views. A host's
// first non-empty frame after it (re)connects is a backlog, not news: it is
// folded into the baseline silently. Ops sends `Connected` before the
// snapshot, so "connected" alone is not the signal; "has sessions" is.
import type { FleetView, Session } from "../ops/types";
import { detectFlips, type Flip } from "./transitions";

export type Baseline = {
  /** Sessions as of the last frame, or null before the first healthy frame. */
  previous: Session[] | null;
  /** Hosts whose backlog has already been folded in. */
  seeded: Set<string>;
};

export const EMPTY_BASELINE: Baseline = { previous: null, seeded: new Set() };

export function advance(baseline: Baseline, view: FleetView | null, current: Session[]): { next: Baseline; flips: Flip[] } {
  const hosts = view?.hosts ?? [];
  const seeded = new Set(baseline.seeded);
  // A host that dropped will deliver a backlog again when it returns.
  for (const h of hosts) if (!h.connected) seeded.delete(h.name);

  if (baseline.previous === null) {
    for (const h of hosts) if (h.connected && current.some((s) => s.host === h.name)) seeded.add(h.name);
    return { next: { previous: current, seeded }, flips: [] };
  }

  let previous = baseline.previous;
  for (const h of hosts) {
    // Ops keeps a dropped host's last sessions; they are not a backlog until it is back.
    if (!h.connected || seeded.has(h.name)) continue;
    const backlog = current.filter((s) => s.host === h.name);
    if (backlog.length === 0) continue;
    previous = [...previous.filter((s) => s.host !== h.name), ...backlog];
    seeded.add(h.name);
  }
  return { next: { previous: current, seeded }, flips: detectFlips(previous, current) };
}
