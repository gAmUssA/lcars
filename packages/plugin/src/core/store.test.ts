import { expect, test } from "vitest";
import { createFleetStore, keyedSessions } from "./store";
import type { FleetView, Session } from "../ops/types";

function session(id: string, agent: string | null, state: Session["state"] = "idle"): Session {
  const [host, pane_id] = id.split("/");
  return {
    id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent, name: null, state,
    title: null, cwd: null, model: null, summary: null, focused: false, seq: 0, updated_at_ms: 0,
  };
}

const view: FleetView = {
  hosts: [],
  sessions: [session("ds9/w7:p1", "claude", "blocked"), session("daystrom/w4:p3", null), session("daystrom/w1:p1", "codex")],
};

test("only panes with an agent get keys, in the order Ops sent", () => {
  expect(keyedSessions(view).map((s) => s.id)).toEqual(["ds9/w7:p1", "daystrom/w1:p1"]);
});

test("setView makes the store healthy and fills slots", () => {
  const store = createFleetStore();
  const seen: boolean[] = [];
  store.subscribe((s) => seen.push(s.healthy));
  expect(store.sessionAt(0)).toBeNull();
  store.setView(view);
  expect(store.getState().healthy).toBe(true);
  expect(store.sessionAt(0)?.id).toBe("ds9/w7:p1");
  expect(store.sessionAt(1)?.id).toBe("daystrom/w1:p1");
  expect(store.sessionAt(2)).toBeNull();
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
