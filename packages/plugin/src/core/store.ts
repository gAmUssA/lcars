// Holds the latest FleetView from Ops plus the plugin-side view state the
// keys and dials change (session and workspace filter, pages, strip mode,
// ticker cursor, effort). Ops already sorts panes (blocked first, then most
// recent); the store decides which panes get a key and which slice is on
// screen. One filter serves every deck: a Session Key on the XL and the
// fleet bar on the Plus set the same thing.
import type { FleetView, Pane, Workspace } from "../ops/types";
import {
  blockedPanes,
  clampPage,
  cycleEffort,
  cycleSession,
  filterKeys,
  pageCount,
  focusedPane,
  sessionNames,
  INITIAL_VIEW,
  pageSlice,
  type Effort,
  type ViewState,
  wallSlot,
  wallPageFor,
  workspacesFor,
} from "./view-state";

export type StoreState = {
  view: FleetView | null;
  /** False until the first view arrives and after Ops goes away. */
  healthy: boolean;
  /** Panes that get a key, after the filters (blocked panes always stay), in Ops's order. */
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
  /** Pane at a wall slot on a deck with `wallSize` wall keys, on the current wall page. */
  paneOnWall(slot: number, wallSize: number): Pane | null;
  /** The wall page a deck with `wallSize` wall keys shows (0-based) and how many it has. */
  wallPage(wallSize: number): { page: number; pages: number };
  nextWallPage(): void;
  /** Workspace at a slot in Ops's order (session, then Herdr number), within the session filter. */
  workspaceAt(slot: number): Workspace | null;
  /** All keyed panes regardless of the session filter. */
  allKeys(): Pane[];
  movePage(step: number): void;
  resetPage(): void;
  cycleSessionFilter(step: number): void;
  setSessionFilter(session: string | null): void;
  /** Filter to `session`, or clear the filter when it is already the one set. */
  toggleSessionFilter(session: string): void;
  /** Narrow to one workspace (`<session>/<workspace_id>`), or clear it when already set. */
  toggleWorkspaceFilter(workspaceId: string): void;
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
    const keys = filterKeys(all, nextUi);
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
      // A filter for a session or workspace that vanished falls back to all.
      const sessionFilter = state.ui.sessionFilter !== null && !sessionNames(view).includes(state.ui.sessionFilter) ? null : state.ui.sessionFilter;
      const workspaceFilter =
        state.ui.workspaceFilter !== null && !view.workspaces.some((w) => w.id === state.ui.workspaceFilter) ? null : state.ui.workspaceFilter;
      recompute({ view, healthy: true }, { sessionFilter, workspaceFilter });
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
    // The wall is "the n-th agent under the filter", paged by the deck's own
    // wall size so the XL and the MK.2 each show whole pages.
    paneOnWall(slot, wallSize) {
      return wallSlot(state.keys, state.ui.wallPage, wallSize, slot);
    },
    wallPage(wallSize) {
      return { page: wallPageFor(state.ui.wallPage, state.keys.length, wallSize), pages: pageCount(state.keys.length, Math.max(wallSize, 1)) };
    },
    nextWallPage() {
      recompute({}, { wallPage: state.ui.wallPage + 1 });
    },
    workspaceAt(slot) {
      return workspacesFor(state.view, state.ui.sessionFilter)[slot] ?? null;
    },
    allKeys: () => all,
    movePage(step) {
      recompute({}, { page: state.ui.page + Math.trunc(step) });
    },
    resetPage() {
      recompute({}, { page: 0 });
    },
    cycleSessionFilter(step) {
      store.setSessionFilter(cycleSession(sessionNames(state.view), state.ui.sessionFilter, step));
    },
    setSessionFilter(session) {
      // A workspace filter belongs to one session; another session drops it.
      const workspaceFilter = session !== null && state.ui.workspaceFilter?.startsWith(`${session}/`) ? state.ui.workspaceFilter : null;
      recompute({}, { sessionFilter: session, workspaceFilter, page: 0, wallPage: 0 });
    },
    toggleSessionFilter(session) {
      store.setSessionFilter(state.ui.sessionFilter === session ? null : session);
    },
    toggleWorkspaceFilter(workspaceId) {
      const workspaceFilter = state.ui.workspaceFilter === workspaceId ? null : workspaceId;
      recompute({}, { workspaceFilter, page: 0, wallPage: 0 });
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
