// Plugin-side view state the dials manipulate: session filter, page, touch
// strip mode, ticker cursor, effort level. Pure functions; the store owns
// the mutable copy.
import type { FleetView, SessionStatus, Pane } from "../ops/types";

export type StripMode = "fleet" | "ticker";

export const EFFORTS = ["low", "medium", "high", "xhigh", "max"] as const;
export type Effort = (typeof EFFORTS)[number];

/** Keys on the Stream Deck + that show panes. */
export const PAGE_SIZE = 8;

export type ViewState = {
  /** Session name, or null for every session. */
  sessionFilter: string | null;
  page: number;
  stripMode: StripMode;
  /** Index into the blocked list shown by the ticker. */
  tickerIndex: number;
  effort: Effort;
};

export const INITIAL_VIEW: ViewState = {
  sessionFilter: null,
  page: 0,
  stripMode: "fleet",
  tickerIndex: 0,
  effort: "high",
};

export function filterBySession(keys: Pane[], sessionFilter: string | null): Pane[] {
  return sessionFilter === null ? keys : keys.filter((s) => s.session === sessionFilter);
}

export function pageCount(count: number, pageSize = PAGE_SIZE): number {
  return Math.max(1, Math.ceil(count / pageSize));
}

export function clampPage(page: number, count: number, pageSize = PAGE_SIZE): number {
  return Math.min(Math.max(0, page), pageCount(count, pageSize) - 1);
}

export function pageSlice(keys: Pane[], page: number, pageSize = PAGE_SIZE): Pane[] {
  const start = clampPage(page, keys.length, pageSize) * pageSize;
  return keys.slice(start, start + pageSize);
}

/** Session names in display order: as Ops lists them, plus any session that only appears on a pane. */
export function sessionNames(view: FleetView | null): string[] {
  const names: string[] = [];
  for (const h of view?.sessions ?? []) if (!names.includes(h.name)) names.push(h.name);
  for (const s of view?.panes ?? []) if (!names.includes(s.session)) names.push(s.session);
  return names;
}

/** Rotate the session filter: null → sessions[0] → … → sessions[n-1] → null. */
export function cycleSession(sessions: string[], current: string | null, step: number): string | null {
  if (sessions.length === 0) return null;
  const ring: (string | null)[] = [null, ...sessions];
  const at = Math.max(0, ring.indexOf(current));
  const next = (((at + Math.trunc(step)) % ring.length) + ring.length) % ring.length;
  return ring[next];
}

export function cycleEffort(current: Effort, step: number): Effort {
  const at = EFFORTS.indexOf(current);
  const next = Math.min(EFFORTS.length - 1, Math.max(0, at + Math.trunc(step)));
  return EFFORTS[next];
}

export function blockedPanes(keys: Pane[]): Pane[] {
  // Oldest blocked first: the one that has waited longest is the one to answer.
  return keys.filter((s) => s.state === "blocked").slice().sort((a, b) => a.updated_at_ms - b.updated_at_ms);
}

/**
 * The pane a pane-level dial acts on: the focused pane on the deck's own
 * session when there is one, otherwise the first focused pane anywhere.
 */
export function focusedPane(keys: Pane[], localSession: string | null): Pane | null {
  const focused = keys.filter((s) => s.focused);
  if (focused.length === 0) return null;
  return focused.find((s) => localSession !== null && s.session === localSession) ?? focused[0];
}

export type SessionSummary = {
  session: SessionStatus | null;
  name: string;
  counts: Record<Pane["state"], number>;
  /** Worst state present, by urgency; null when the session has no panes. */
  worst: Pane["state"] | null;
};

const URGENCY: Record<Pane["state"], number> = { blocked: 0, done: 1, working: 2, idle: 3, unknown: 4 };

export function summariseSessions(view: FleetView | null, keys: Pane[]): SessionSummary[] {
  return sessionNames(view).map((name) => {
    const counts: Record<Pane["state"], number> = { blocked: 0, done: 0, working: 0, idle: 0, unknown: 0 };
    let worst: Pane["state"] | null = null;
    for (const s of keys) {
      if (s.session !== name) continue;
      counts[s.state] += 1;
      if (worst === null || URGENCY[s.state] < URGENCY[worst]) worst = s.state;
    }
    return { session: view?.sessions.find((h) => h.name === name) ?? null, name, counts, worst };
  });
}
