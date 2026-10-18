// Holds the latest FleetView from Ops plus the plugin-side view state the
// dials change (host filter, page, strip mode, ticker cursor, effort).
// Ops already sorts sessions (blocked first, then most recent); the store
// decides which sessions get a key and which slice is on screen.
import type { FleetView, Session } from "../ops/types";
import {
  blockedSessions,
  clampPage,
  cycleEffort,
  cycleHost,
  filterByHost,
  focusedSession,
  hostNames,
  INITIAL_VIEW,
  pageSlice,
  type Effort,
  type ViewState,
} from "./view-state";

export type StoreState = {
  view: FleetView | null;
  /** False until the first view arrives and after Ops goes away. */
  healthy: boolean;
  /** Sessions that get a key, after the host filter, in Ops's order. */
  keys: Session[];
  ui: ViewState;
};

export type FleetStore = {
  getState(): StoreState;
  subscribe(fn: (s: StoreState) => void): () => void;
  setView(view: FleetView): void;
  setDown(): void;
  /** Session shown in a slot of the current page, or null when the slot is empty. */
  sessionAt(slot: number): Session | null;
  /** Session at an absolute slot over the whole filtered list (wall layout), ignoring paging. */
  sessionAtAbsolute(slot: number): Session | null;
  /** All keyed sessions regardless of the host filter. */
  allKeys(): Session[];
  movePage(step: number): void;
  resetPage(): void;
  cycleHostFilter(step: number): void;
  setHostFilter(host: string | null): void;
  toggleStripMode(): void;
  moveTicker(step: number): void;
  tickerSession(): Session | null;
  setEffort(effort: Effort): void;
  cycleEffortLevel(step: number): void;
  focusedSession(): Session | null;
};

// Keys show panes with a recognised agent. Plain shells have no state worth a
// key and would push real agents off an 8-key deck.
export function keyedSessions(view: FleetView): Session[] {
  return view.sessions.filter((s) => s.agent !== null);
}

export function createFleetStore(opts: { localHost?: string | null } = {}): FleetStore {
  const localHost = opts.localHost ?? null;
  let all: Session[] = [];
  let state: StoreState = { view: null, healthy: false, keys: [], ui: { ...INITIAL_VIEW } };
  const subs = new Set<(s: StoreState) => void>();
  const emit = (): void => subs.forEach((fn) => fn(state));

  const recompute = (patch: Partial<StoreState> = {}, ui: Partial<ViewState> = {}): void => {
    const nextUi = { ...state.ui, ...ui };
    const keys = filterByHost(all, nextUi.hostFilter);
    nextUi.page = clampPage(nextUi.page, keys.length);
    const blocked = blockedSessions(keys);
    nextUi.tickerIndex = blocked.length === 0 ? 0 : ((nextUi.tickerIndex % blocked.length) + blocked.length) % blocked.length;
    state = { ...state, ...patch, keys, ui: nextUi };
    emit();
  };

  const store: FleetStore = {
    getState: () => state,
    subscribe(fn) {
      subs.add(fn);
      fn(state);
      return () => {
        subs.delete(fn);
      };
    },
    setView(view) {
      all = keyedSessions(view);
      // A filter for a host that vanished falls back to all hosts.
      const hostFilter = state.ui.hostFilter !== null && !hostNames(view).includes(state.ui.hostFilter) ? null : state.ui.hostFilter;
      recompute({ view, healthy: true }, { hostFilter });
    },
    setDown() {
      if (!state.healthy && state.view === null) return;
      // Keep the last keys so a flicker of Ops does not wipe the deck; the
      // renderer dims them while unhealthy.
      state = { ...state, healthy: false };
      emit();
    },
    sessionAt(slot) {
      return pageSlice(state.keys, state.ui.page)[slot] ?? null;
    },
    // The wall is "the n-th agent across the fleet": the Plus host filter
    // must not rewrite it.
    sessionAtAbsolute(slot) {
      return all[slot] ?? null;
    },
    allKeys: () => all,
    movePage(step) {
      recompute({}, { page: state.ui.page + Math.trunc(step) });
    },
    resetPage() {
      recompute({}, { page: 0 });
    },
    cycleHostFilter(step) {
      recompute({}, { hostFilter: cycleHost(hostNames(state.view), state.ui.hostFilter, step), page: 0 });
    },
    setHostFilter(host) {
      recompute({}, { hostFilter: host, page: 0 });
    },
    toggleStripMode() {
      recompute({}, { stripMode: state.ui.stripMode === "fleet" ? "ticker" : "fleet" });
    },
    moveTicker(step) {
      recompute({}, { tickerIndex: state.ui.tickerIndex + Math.trunc(step) });
    },
    tickerSession() {
      const blocked = blockedSessions(state.keys);
      return blocked[state.ui.tickerIndex] ?? null;
    },
    setEffort(effort) {
      recompute({}, { effort });
    },
    cycleEffortLevel(step) {
      recompute({}, { effort: cycleEffort(state.ui.effort, step) });
    },
    focusedSession() {
      // Ops marks the host whose Herdr socket is on this machine; fall back
      // to the configured name only when no view has arrived yet.
      const local = state.view?.hosts.find((h) => h.local)?.name ?? localHost;
      return focusedSession(all, local);
    },
  };
  return store;
}
