import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { busySessions, GLOBAL_UUIDS, STOP_KEYS, StopAllAction } from "./global-keys";
import type { FleetView, Session } from "../ops/types";

function s(id: string, state: Session["state"]): Session {
  const [host, pane_id] = id.split("/");
  return {
    id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state,
    title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0,
  };
}

test("every global key uuid is in the manifest as a Keypad action", () => {
  const uuids = manifest.Actions.filter((a) => a.Controllers.includes("Keypad")).map((a) => a.UUID);
  for (const uuid of Object.values(GLOBAL_UUIDS)) expect(uuids).toContain(uuid);
});

test("busy sessions are the working and blocked ones", () => {
  const keys = [s("h/p1", "working"), s("h/p2", "idle"), s("h/p3", "blocked"), s("h/p4", "unknown")];
  expect(busySessions(keys).map((k) => k.id)).toEqual(["h/p1", "h/p3"]);
  expect(STOP_KEYS).toEqual(["esc"]);
});

test("stop all needs two presses and then interrupts every busy agent", async () => {
  const store = createFleetStore();
  const view: FleetView = { workspaces: [], hosts: [], sessions: [s("h/p1", "working"), s("h/p2", "idle"), s("h/p3", "blocked")] };
  store.setView(view);
  const sent: [string, string[]][] = [];
  const ops = { focus: async () => {}, scroll: async () => 0, prompt: async () => {}, sendKeys: async (id: string, keys: string[]) => { sent.push([id, keys]); }, focusWorkspace: async () => {} };
  let t = 0;
  const action = new StopAllAction(store, ops, () => t);
  const ev = { action: { showAlert: async () => {} } } as never;
  await action.onKeyDown(ev);
  expect(action.trigger.isArmed()).toBe(true);
  expect(sent).toEqual([]);
  t = 1000;
  await action.onKeyDown(ev);
  expect(sent).toEqual([
    ["h/p1", ["esc"]],
    ["h/p3", ["esc"]],
  ]);
  expect(action.trigger.isArmed()).toBe(false);
});
