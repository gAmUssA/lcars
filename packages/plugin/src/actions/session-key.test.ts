import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { FILTER_COLOR, renderSessionKey } from "../core/render";
import { SESSION_KEY_UUID, SessionKeyAction, sessionAt, sessionSlot } from "./session-key";
import type { FleetView, Pane, SessionStatus } from "../ops/types";

function pane(id: string, state: Pane["state"] = "idle"): Pane {
  const [session, pane_id] = id.split("/");
  return { id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state, title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0 };
}
const status = (name: string, machine: string | null, connected = true): SessionStatus => ({
  name, machine, local: machine === null, connected, version: "0.9.0", protocol: 22, error: null, updated_at_ms: 0,
});
const view: FleetView = {
  workspaces: [],
  sessions: [status("mini", "ds9"), status("studio", null), status("talk", "ds9")],
  panes: [pane("talk/w4:p1", "blocked"), pane("studio/w1:p1", "working"), pane("talk/w1:p2"), pane("talk/w1:p3")],
};
const svg = (uri: string): string => decodeURIComponent(uri.slice("data:image/svg+xml,".length));

test("the action is in the manifest and slots parse like the other keys", () => {
  expect(manifest.Actions.map((a) => a.UUID)).toContain(SESSION_KEY_UUID);
  expect(sessionSlot({})).toBe(0);
  expect(sessionSlot({ slotIndex: "2" })).toBe(2);
  expect(sessionSlot({ slotIndex: "x" })).toBe(0);
});

test("sessions come in Ops order with counts over every agent, whatever the filter", () => {
  const store = createFleetStore();
  expect(sessionAt(store, 0)).toBeNull();
  store.setView(view);
  store.setSessionFilter("studio");
  expect([0, 1, 2].map((i) => sessionAt(store, i)?.name)).toEqual(["mini", "studio", "talk"]);
  const talk = sessionAt(store, 2)!;
  expect(talk.worst).toBe("blocked");
  expect(talk.counts).toMatchObject({ blocked: 1, idle: 2 });
  expect(talk.session?.machine).toBe("ds9");
  expect(sessionAt(store, 3)).toBeNull();
});

test("session key shows the machine, worst colour, blocked count, filter ring and page", () => {
  const busy = svg(renderSessionKey({ name: "talk", machine: "ds9", worst: "blocked", agents: 3, blocked: 1, connected: true, selected: true, page: "1/2", live: true }));
  expect(busy).toContain("#D13438");
  expect(busy).toContain(">DS9<");
  expect(busy).toContain(">talk<");
  expect(busy).toContain(">1 blocked<");
  expect(busy).toContain(`stroke="${FILTER_COLOR}"`);
  expect(busy).toContain(">1/2<");
  const local = svg(renderSessionKey({ name: "studio", machine: null, worst: "idle", agents: 1, blocked: 0, connected: true, selected: false, page: null, live: true }));
  expect(local).toContain(">LOCAL<");
  expect(local).toContain(">1 agent<");
  expect(local).not.toContain(FILTER_COLOR);
  expect(local).not.toContain('fill-opacity="0.6"');
  const down = svg(renderSessionKey({ name: "mini", machine: "ds9", worst: "idle", agents: 1, blocked: 0, connected: false, selected: false, page: null, live: true }));
  expect(down).toContain(">offline<");
  expect(down).toContain('fill-opacity="0.6"');
});

test("tap toggles the session filter, long press turns the wall page, an empty slot alerts", async () => {
  const store = createFleetStore();
  store.setView({ ...view, panes: [...view.panes, ...Array.from({ length: 8 }, (_, i) => pane(`studio/w1:x${i}`))] });
  let now = 0;
  const action = new SessionKeyAction(store, () => 6, () => now);
  const alerts: string[] = [];
  const ev = (slotIndex: string) => ({ action: { id: "s1", device: { id: "d1" }, showAlert: async () => { alerts.push(slotIndex); } }, payload: { settings: { slotIndex } } }) as never;

  action.onKeyDown(ev("2"));
  now += 100;
  await action.onKeyUp(ev("2"));
  expect(store.getState().ui.sessionFilter).toBe("talk");
  action.onKeyDown(ev("2"));
  now += 100;
  await action.onKeyUp(ev("2"));
  expect(store.getState().ui.sessionFilter).toBeNull();

  // 12 agents on a 6-key wall: two pages; a long press turns to the second.
  expect(store.wallPage(6)).toEqual({ page: 0, pages: 2 });
  action.onKeyDown(ev("0"));
  now += 500;
  await action.onKeyUp(ev("0"));
  expect(store.wallPage(6)).toEqual({ page: 1, pages: 2 });
  expect(store.getState().ui.sessionFilter).toBeNull();

  action.onKeyDown(ev("7"));
  now += 100;
  await action.onKeyUp(ev("7"));
  expect(alerts).toEqual(["7"]);
});
