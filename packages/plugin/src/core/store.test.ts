import { expect, test } from "vitest";
import { createFleetStore, keyedPanes } from "./store";
import type { FleetView, Pane } from "../ops/types";

function pane(id: string, agent: string | null, over: Partial<Pane> = {}): Pane {
  const [session, pane_id] = id.split("/");
  return {
    id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent, name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
  };
}

const online = { machine: null, local: false, connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 };
const view: FleetView = {
  workspaces: [],
  sessions: [{ name: "studio", ...online }, { name: "mini", ...online }],
  panes: [
    pane("mini/w7:p1", "claude", { state: "blocked", updated_at_ms: 5 }),
    pane("studio/w4:p3", null),
    pane("studio/w1:p1", "codex", { focused: true }),
    pane("studio/w1:p2", "claude", { state: "blocked", updated_at_ms: 1 }),
  ],
};

test("only panes with an agent get keys, in the order Ops sent", () => {
  expect(keyedPanes(view).map((s) => s.id)).toEqual(["mini/w7:p1", "studio/w1:p1", "studio/w1:p2"]);
});

test("setView makes the store healthy and fills slots", () => {
  const store = createFleetStore();
  const seen: boolean[] = [];
  store.subscribe((s) => seen.push(s.healthy));
  expect(store.paneAt(0)).toBeNull();
  store.setView(view);
  expect(store.getState().healthy).toBe(true);
  expect(store.paneAt(0)?.id).toBe("mini/w7:p1");
  expect(store.paneAt(2)?.id).toBe("studio/w1:p2");
  expect(store.paneAt(3)).toBeNull();
  expect(seen).toEqual([false, true]);
});

test("setDown keeps the last keys but flags the store unhealthy", () => {
  const store = createFleetStore();
  store.setView(view);
  store.setDown();
  expect(store.getState().healthy).toBe(false);
  expect(store.paneAt(0)?.id).toBe("mini/w7:p1");
});

test("setDown before any view emits nothing", () => {
  const store = createFleetStore();
  let emits = 0;
  store.subscribe(() => emits++);
  store.setDown();
  expect(emits).toBe(1); // only the initial subscribe emit
});

test("session filter narrows the keys and survives a view update, falls back when the session vanishes", () => {
  const store = createFleetStore();
  store.setView(view);
  store.cycleSessionFilter(1);
  expect(store.getState().ui.sessionFilter).toBe("studio");
  expect(store.getState().keys.map((s) => s.id)).toEqual(["studio/w1:p1", "studio/w1:p2"]);
  store.setView(view);
  expect(store.getState().ui.sessionFilter).toBe("studio");
  store.setSessionFilter("mini");
  expect(store.paneAt(0)?.id).toBe("mini/w7:p1");
  store.setView({ workspaces: [], sessions: [view.sessions[0]], panes: view.panes.filter((s) => s.session === "studio") });
  expect(store.getState().ui.sessionFilter).toBeNull();
  expect(store.allKeys()).toHaveLength(2);
});

test("paging moves through keys and resets", () => {
  const store = createFleetStore();
  const many: FleetView = {
    workspaces: [],
    sessions: [],
    panes: Array.from({ length: 11 }, (_, i) => pane(`h/p${i}`, "claude")),
  };
  store.setView(many);
  expect(store.paneAt(0)?.pane_id).toBe("p0");
  store.movePage(1);
  expect(store.getState().ui.page).toBe(1);
  expect(store.paneAt(0)?.pane_id).toBe("p8");
  // Wall slots ignore the page.
  expect(store.paneAtAbsolute(0)?.pane_id).toBe("p0");
  expect(store.paneAtAbsolute(10)?.pane_id).toBe("p10");
  expect(store.paneAtAbsolute(11)).toBeNull();
  store.movePage(3);
  expect(store.getState().ui.page).toBe(1); // clamped
  store.resetPage();
  expect(store.paneAt(0)?.pane_id).toBe("p0");
});

test("the wall ignores the session filter and the page", () => {
  const store = createFleetStore();
  store.setView(view);
  expect(store.paneAtAbsolute(0)?.id).toBe("mini/w7:p1");
  store.setSessionFilter("studio");
  expect(store.paneAt(0)?.id).toBe("studio/w1:p1");
  expect(store.paneAtAbsolute(0)?.id).toBe("mini/w7:p1");
  expect(store.paneAtAbsolute(2)?.id).toBe("studio/w1:p2");
});

test("focused pane follows the session Ops marks local, whatever the machine name is", () => {
  const store = createFleetStore({ localSession: "mac-studio" });
  const marked: FleetView = {
    workspaces: [],
    sessions: [{ ...view.sessions[0], machine: null, local: true }, view.sessions[1]],
    panes: [pane("mini/w7:p1", "claude", { focused: true, state: "blocked" }), pane("studio/w1:p1", "codex", { focused: true })],
  };
  store.setView(marked);
  expect(store.focusedPane()?.id).toBe("studio/w1:p1");
});

test("strip mode toggles and the ticker cycles through blocked panes oldest first", () => {
  const store = createFleetStore();
  store.setView(view);
  expect(store.getState().ui.stripMode).toBe("fleet");
  store.toggleStripMode();
  expect(store.getState().ui.stripMode).toBe("ticker");
  expect(store.tickerPane()?.id).toBe("studio/w1:p2");
  store.moveTicker(1);
  expect(store.tickerPane()?.id).toBe("mini/w7:p1");
  store.moveTicker(1);
  expect(store.tickerPane()?.id).toBe("studio/w1:p2"); // wraps
  store.moveTicker(-1);
  expect(store.tickerPane()?.id).toBe("mini/w7:p1");
});

test("effort level is tracked and the focused pane prefers the local session", () => {
  const store = createFleetStore({ localSession: "studio" });
  store.setView(view);
  expect(store.getState().ui.effort).toBe("high");
  store.cycleEffortLevel(1);
  expect(store.getState().ui.effort).toBe("xhigh");
  store.setEffort("low");
  expect(store.getState().ui.effort).toBe("low");
  expect(store.focusedPane()?.id).toBe("studio/w1:p1");
});
