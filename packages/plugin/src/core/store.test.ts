import { expect, test } from "vitest";
import { createFleetStore, keyedSessions } from "./store";
import type { FleetView, Session } from "../ops/types";

function session(id: string, agent: string | null, over: Partial<Session> = {}): Session {
  const [host, pane_id] = id.split("/");
  return {
    id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent, name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
  };
}

const online = { connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 };
const view: FleetView = {
  hosts: [{ name: "daystrom", ...online }, { name: "ds9", ...online }],
  sessions: [
    session("ds9/w7:p1", "claude", { state: "blocked", updated_at_ms: 5 }),
    session("daystrom/w4:p3", null),
    session("daystrom/w1:p1", "codex", { focused: true }),
    session("daystrom/w1:p2", "claude", { state: "blocked", updated_at_ms: 1 }),
  ],
};

test("only panes with an agent get keys, in the order Ops sent", () => {
  expect(keyedSessions(view).map((s) => s.id)).toEqual(["ds9/w7:p1", "daystrom/w1:p1", "daystrom/w1:p2"]);
});

test("setView makes the store healthy and fills slots", () => {
  const store = createFleetStore();
  const seen: boolean[] = [];
  store.subscribe((s) => seen.push(s.healthy));
  expect(store.sessionAt(0)).toBeNull();
  store.setView(view);
  expect(store.getState().healthy).toBe(true);
  expect(store.sessionAt(0)?.id).toBe("ds9/w7:p1");
  expect(store.sessionAt(2)?.id).toBe("daystrom/w1:p2");
  expect(store.sessionAt(3)).toBeNull();
  expect(seen).toEqual([false, true]);
});

test("setDown keeps the last keys but flags the store unhealthy", () => {
  const store = createFleetStore();
  store.setView(view);
  store.setDown();
  expect(store.getState().healthy).toBe(false);
  expect(store.sessionAt(0)?.id).toBe("ds9/w7:p1");
});

test("setDown before any view emits nothing", () => {
  const store = createFleetStore();
  let emits = 0;
  store.subscribe(() => emits++);
  store.setDown();
  expect(emits).toBe(1); // only the initial subscribe emit
});

test("host filter narrows the keys and survives a view update, falls back when the host vanishes", () => {
  const store = createFleetStore();
  store.setView(view);
  store.cycleHostFilter(1);
  expect(store.getState().ui.hostFilter).toBe("daystrom");
  expect(store.getState().keys.map((s) => s.id)).toEqual(["daystrom/w1:p1", "daystrom/w1:p2"]);
  store.setView(view);
  expect(store.getState().ui.hostFilter).toBe("daystrom");
  store.setHostFilter("ds9");
  expect(store.sessionAt(0)?.id).toBe("ds9/w7:p1");
  store.setView({ hosts: [view.hosts[0]], sessions: view.sessions.filter((s) => s.host === "daystrom") });
  expect(store.getState().ui.hostFilter).toBeNull();
  expect(store.allKeys()).toHaveLength(2);
});

test("paging moves through keys and resets", () => {
  const store = createFleetStore();
  const many: FleetView = {
    hosts: [],
    sessions: Array.from({ length: 11 }, (_, i) => session(`h/p${i}`, "claude")),
  };
  store.setView(many);
  expect(store.sessionAt(0)?.pane_id).toBe("p0");
  store.movePage(1);
  expect(store.getState().ui.page).toBe(1);
  expect(store.sessionAt(0)?.pane_id).toBe("p8");
  // Wall slots ignore the page.
  expect(store.sessionAtAbsolute(0)?.pane_id).toBe("p0");
  expect(store.sessionAtAbsolute(10)?.pane_id).toBe("p10");
  expect(store.sessionAtAbsolute(11)).toBeNull();
  store.movePage(3);
  expect(store.getState().ui.page).toBe(1); // clamped
  store.resetPage();
  expect(store.sessionAt(0)?.pane_id).toBe("p0");
});

test("strip mode toggles and the ticker cycles through blocked sessions oldest first", () => {
  const store = createFleetStore();
  store.setView(view);
  expect(store.getState().ui.stripMode).toBe("fleet");
  store.toggleStripMode();
  expect(store.getState().ui.stripMode).toBe("ticker");
  expect(store.tickerSession()?.id).toBe("daystrom/w1:p2");
  store.moveTicker(1);
  expect(store.tickerSession()?.id).toBe("ds9/w7:p1");
  store.moveTicker(1);
  expect(store.tickerSession()?.id).toBe("daystrom/w1:p2"); // wraps
  store.moveTicker(-1);
  expect(store.tickerSession()?.id).toBe("ds9/w7:p1");
});

test("effort level is tracked and the focused session prefers the local host", () => {
  const store = createFleetStore({ localHost: "daystrom" });
  store.setView(view);
  expect(store.getState().ui.effort).toBe("high");
  store.cycleEffortLevel(1);
  expect(store.getState().ui.effort).toBe("xhigh");
  store.setEffort("low");
  expect(store.getState().ui.effort).toBe("low");
  expect(store.focusedSession()?.id).toBe("daystrom/w1:p1");
});
