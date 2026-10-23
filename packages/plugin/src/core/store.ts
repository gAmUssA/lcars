// Holds the latest FleetView from Ops plus the plugin-side view state the
// dials change (session filter, page, strip mode, ticker cursor, effort).
// Ops already sorts panes (blocked first, then most recent); the store
// decides which panes get a key and which slice is on screen.
import type { FleetView, Pane, Workspace } from "../ops/types";
import {
  blockedPanes,
  clampPage,
  cycleEffort,
  cycleSession,
  filterBySession,
  focusedPane,
  sessionNames,
  INITIAL_VIEW,
  pageSlice,
  type Effort,
  type ViewState,
} from "./view-state";

export type StoreState = {
  view: FleetView | null;
  /** False until the first view arrives and after Ops goes away. */
  healthy: boolean;
  /** Panes that get a key, after the session filter, in Ops's order. */
  keys: Pane[];
  ui: ViewState;
};

export type FleetStore = {
  getState(): StoreState;
  subscribe(fn: (s: StoreState) => void): () => void;
  setView(view: FleetView): void;
  setDown(): void;
  /** Pane shown in a slot of the current page, or null when the slot is empty. */
  paneAt(slot: number): Pane | null;
  /** Pane at an absolute slot over the whole filtered list (wall layout), ignoring paging. */
  paneAtAbsolute(slot: number): Pane | null;
  /** Workspace at a slot in Ops's order (session, then Herdr number); unfiltered. */
  workspaceAt(slot: number): Workspace | null;
  /** All keyed panes regardless of the session filter. */
  allKeys(): Pane[];
  movePage(step: number): void;
  resetPage(): void;
  cycleSessionFilter(step: number): void;
  setSessionFilter(session: string | null): void;
  toggleStripMode(): void;
  moveTicker(step: number): void;
  tickerPane(): Pane | null;
  setEffort(effort: Effort): void;
  cycleEffortLevel(step: number): void;
  focusedPane(): Pane | null;
};

// Keys show panes with a recognised agent. Plain shells have no state worth a
// key and would push real agents off an 8-key deck.
export function keyedPanes(view: FleetView): Pane[] {
  return view.panes.filter((s) => s.agent !== null);
}

export function createFleetStore(opts: { localSession?: string | null } = {}): FleetStore {
  const localSession = opts.localSession ?? null;
  let all: Pane[] = [];
  let state: StoreState = { view: null, healthy: false, keys: [], ui: { ...INITIAL_VIEW } };
  const subs = new Set<(s: StoreState) => void>();
  const emit = (): void => subs.forEach((fn) => fn(state));

  const recompute = (patch: Partial<StoreState> = {}, ui: Partial<ViewState> = {}): void => {
    const nextUi = { ...state.ui, ...ui };
    const keys = filterBySession(all, nextUi.sessionFilter);
    nextUi.page = clampPage(nextUi.page, keys.length);
    const blocked = blockedPanes(keys);
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
      all = keyedPanes(view);
      // A filter for a session that vanished falls back to all sessions.
      const sessionFilter = state.ui.sessionFilter !== null && !sessionNames(view).includes(state.ui.sessionFilter) ? null : state.ui.sessionFilter;
      recompute({ view, healthy: true }, { sessionFilter });
    },
    setDown() {
      if (!state.healthy && state.view === null) return;
      // Keep the last keys so a flicker of Ops does not wipe the deck; the
      // renderer dims them while unhealthy.
      state = { ...state, healthy: false };
      emit();
    },
    paneAt(slot) {
      return pageSlice(state.keys, state.ui.page)[slot] ?? null;
    },
    // The wall is "the n-th agent across the fleet": the Plus session filter
    // must not rewrite it.
    paneAtAbsolute(slot) {
      return all[slot] ?? null;
    },
    workspaceAt(slot) {
      return state.view?.workspaces[slot] ?? null;
    },
    allKeys: () => all,
    movePage(step) {
      recompute({}, { page: state.ui.page + Math.trunc(step) });
    },
    resetPage() {
      recompute({}, { page: 0 });
    },
    cycleSessionFilter(step) {
      recompute({}, { sessionFilter: cycleSession(sessionNames(state.view), state.ui.sessionFilter, step), page: 0 });
    },
    setSessionFilter(session) {
      recompute({}, { sessionFilter: session, page: 0 });
    },
    toggleStripMode() {
      recompute({}, { stripMode: state.ui.stripMode === "fleet" ? "ticker" : "fleet" });
    },
    moveTicker(step) {
      recompute({}, { tickerIndex: state.ui.tickerIndex + Math.trunc(step) });
    },
    tickerPane() {
      const blocked = blockedPanes(state.keys);
      return blocked[state.ui.tickerIndex] ?? null;
    },
    setEffort(effort) {
      recompute({}, { effort });
    },
    cycleEffortLevel(step) {
      recompute({}, { effort: cycleEffort(state.ui.effort, step) });
    },
    focusedPane() {
      // Ops marks the session whose Herdr socket is on this machine; fall back
      // to the configured name only when no view has arrived yet.
      const local = state.view?.sessions.find((h) => h.local)?.name ?? localSession;
      return focusedPane(all, local);
    },
  };
  return store;
}
