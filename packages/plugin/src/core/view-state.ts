// Plugin-side view state the keys and dials manipulate: session and
// workspace filter, Plus page, wall page, touch strip mode, ticker cursor,
// effort level. Pure functions; the store owns the mutable copy.
import type { FleetView, SessionStatus, Pane, Workspace } from "../ops/types";

export type StripMode = "fleet" | "ticker";

export const EFFORTS = ["low", "medium", "high", "xhigh", "max"] as const;
export type Effort = (typeof EFFORTS)[number];

/** Keys on the Stream Deck + that show panes. */
export const PAGE_SIZE = 8;

export type ViewState = {
  /** Herdr session name, or null for every session. */
  sessionFilter: string | null;
  /** Workspace id (`<session>/<workspace_id>`), or null for every workspace. */
  workspaceFilter: string | null;
  /** Plus page (8 keys per page). */
  page: number;
  /** Wall page counter; each deck wraps it by its own wall size. */
  wallPage: number;
  stripMode: StripMode;
  /** Index into the blocked list shown by the ticker. */
  tickerIndex: number;
  effort: Effort;
};

export const INITIAL_VIEW: ViewState = {
  sessionFilter: null,
  workspaceFilter: null,
  page: 0,
  wallPage: 0,
  stripMode: "fleet",
  tickerIndex: 0,
  effort: "high",
};

/** Workspace id of a pane, as Ops spells workspace ids. */
export function workspaceIdOf(pane: Pane): string {
  return `${pane.session}/${pane.workspace_id}`;
}

export type Filter = { sessionFilter: string | null; workspaceFilter: string | null };

export function matchesFilter(pane: Pane, filter: Filter): boolean {
  if (filter.sessionFilter !== null && pane.session !== filter.sessionFilter) return false;
  if (filter.workspaceFilter !== null && workspaceIdOf(pane) !== filter.workspaceFilter) return false;
  return true;
}

/**
 * Panes that get a key under a filter. A blocked agent outside the filter
 * stays: it needs the operator, and a filter must never hide a question.
 * Ops sends blocked panes first, so pinned ones lead the list.
 */
export function filterKeys(keys: Pane[], filter: Filter): Pane[] {
  return keys.filter((p) => p.state === "blocked" || matchesFilter(p, filter));
}

/** Workspaces the Workspace Keys show: one session's under a session filter, else all. */
export function workspacesFor(view: FleetView | null, sessionFilter: string | null): Workspace[] {
  const all = view?.workspaces ?? [];
  return sessionFilter === null ? all : all.filter((w) => w.session === sessionFilter);
}

/** The wall page a deck with `wallSize` wall keys shows: the shared counter, wrapped. */
export function wallPageFor(wallPage: number, count: number, wallSize: number): number {
  if (wallSize <= 0) return 0;
  const pages = pageCount(count, wallSize);
  return ((Math.trunc(wallPage) % pages) + pages) % pages;
}

/** Pane at a wall slot on a deck with `wallSize` wall keys. */
export function wallSlot(keys: Pane[], wallPage: number, wallSize: number, slot: number): Pane | null {
  if (wallSize <= 0) return keys[slot] ?? null;
  return keys[wallPageFor(wallPage, keys.length, wallSize) * wallSize + slot] ?? null;
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
