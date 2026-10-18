// Holds the latest FleetView from Ops and the derived key list. Ops already
// sorts sessions (blocked first, then most recent); the store only decides
// which sessions get a key.
import type { FleetView, Session } from "../ops/types";

export type StoreState = {
  view: FleetView | null;
  /** False until the first view arrives and after Ops goes away. */
  healthy: boolean;
  /** Sessions that get a key, in slot order. */
  keys: Session[];
};

export type FleetStore = {
  getState(): StoreState;
  subscribe(fn: (s: StoreState) => void): () => void;
  setView(view: FleetView): void;
  setDown(): void;
  /** Session shown in a slot, or null when the slot is empty. */
  sessionAt(slot: number): Session | null;
};

// Keys show panes with a recognised agent. Plain shells have no state worth a
// key and would push real agents off an 8-key deck.
export function keyedSessions(view: FleetView): Session[] {
  return view.sessions.filter((s) => s.agent !== null);
}

export function createFleetStore(): FleetStore {
  let state: StoreState = { view: null, healthy: false, keys: [] };
  const subs = new Set<(s: StoreState) => void>();
  const emit = (): void => subs.forEach((fn) => fn(state));

  return {
    getState: () => state,
    subscribe(fn) {
      subs.add(fn);
      fn(state);
      return () => {
        subs.delete(fn);
      };
    },
    setView(view) {
      state = { view, healthy: true, keys: keyedSessions(view) };
      emit();
    },
    setDown() {
      if (!state.healthy && state.view === null) return;
      // Keep the last keys so a flicker of Ops does not wipe the deck; the
      // renderer dims them while unhealthy.
      state = { ...state, healthy: false };
      emit();
    },
    sessionAt(slot) {
      return state.keys[slot] ?? null;
    },
  };
}
