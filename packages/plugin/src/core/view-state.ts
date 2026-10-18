// Plugin-side view state the dials manipulate: host filter, page, touch
// strip mode, ticker cursor, effort level. Pure functions; the store owns
// the mutable copy.
import type { FleetView, HostStatus, Session } from "../ops/types";

export type StripMode = "fleet" | "ticker";

export const EFFORTS = ["low", "medium", "high", "xhigh", "max"] as const;
export type Effort = (typeof EFFORTS)[number];

/** Keys on the Stream Deck + that show sessions. */
export const PAGE_SIZE = 8;

export type ViewState = {
  /** Host name, or null for every host. */
  hostFilter: string | null;
  page: number;
  stripMode: StripMode;
  /** Index into the blocked list shown by the ticker. */
  tickerIndex: number;
  effort: Effort;
};

export const INITIAL_VIEW: ViewState = {
  hostFilter: null,
  page: 0,
  stripMode: "fleet",
  tickerIndex: 0,
  effort: "high",
};

export function filterByHost(keys: Session[], hostFilter: string | null): Session[] {
  return hostFilter === null ? keys : keys.filter((s) => s.host === hostFilter);
}

export function pageCount(count: number, pageSize = PAGE_SIZE): number {
  return Math.max(1, Math.ceil(count / pageSize));
}

export function clampPage(page: number, count: number, pageSize = PAGE_SIZE): number {
  return Math.min(Math.max(0, page), pageCount(count, pageSize) - 1);
}

export function pageSlice(keys: Session[], page: number, pageSize = PAGE_SIZE): Session[] {
  const start = clampPage(page, keys.length, pageSize) * pageSize;
  return keys.slice(start, start + pageSize);
}

/** Host names in display order: as Ops lists them, plus any host that only appears on a session. */
export function hostNames(view: FleetView | null): string[] {
  const names: string[] = [];
  for (const h of view?.hosts ?? []) if (!names.includes(h.name)) names.push(h.name);
  for (const s of view?.sessions ?? []) if (!names.includes(s.host)) names.push(s.host);
  return names;
}

/** Rotate the host filter: null → hosts[0] → … → hosts[n-1] → null. */
export function cycleHost(hosts: string[], current: string | null, step: number): string | null {
  if (hosts.length === 0) return null;
  const ring: (string | null)[] = [null, ...hosts];
  const at = Math.max(0, ring.indexOf(current));
  const next = (((at + Math.sign(step)) % ring.length) + ring.length) % ring.length;
  return ring[next];
}

export function cycleEffort(current: Effort, step: number): Effort {
  const at = EFFORTS.indexOf(current);
  const next = Math.min(EFFORTS.length - 1, Math.max(0, at + Math.sign(step)));
  return EFFORTS[next];
}

export function blockedSessions(keys: Session[]): Session[] {
  // Oldest blocked first: the one that has waited longest is the one to answer.
  return keys.filter((s) => s.state === "blocked").slice().sort((a, b) => a.updated_at_ms - b.updated_at_ms);
}

/**
 * The session a pane-level dial acts on: the focused pane on the deck's own
 * host when there is one, otherwise the first focused pane anywhere.
 */
export function focusedSession(keys: Session[], localHost: string | null): Session | null {
  const focused = keys.filter((s) => s.focused);
  if (focused.length === 0) return null;
  return focused.find((s) => localHost !== null && s.host === localHost) ?? focused[0];
}

export type HostSummary = {
  host: HostStatus | null;
  name: string;
  counts: Record<Session["state"], number>;
  /** Worst state present, by urgency; null when the host has no sessions. */
  worst: Session["state"] | null;
};

const URGENCY: Record<Session["state"], number> = { blocked: 0, done: 1, working: 2, idle: 3, unknown: 4 };

export function summariseHosts(view: FleetView | null, keys: Session[]): HostSummary[] {
  return hostNames(view).map((name) => {
    const counts: Record<Session["state"], number> = { blocked: 0, done: 0, working: 0, idle: 0, unknown: 0 };
    let worst: Session["state"] | null = null;
    for (const s of keys) {
      if (s.host !== name) continue;
      counts[s.state] += 1;
      if (worst === null || URGENCY[s.state] < URGENCY[worst]) worst = s.state;
    }
    return { host: view?.hosts.find((h) => h.name === name) ?? null, name, counts, worst };
  });
}
