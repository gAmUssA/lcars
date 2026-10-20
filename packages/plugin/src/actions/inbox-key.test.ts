import { expect, test } from "vitest";
import { CONN_KEYS, INBOX_KEY_UUID, InboxKeyAction, LONG_PRESS_MS, slotConfig } from "./inbox-key";
import { createFleetStore } from "../core/store";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";

test("slot settings default to slot 0, title labels, paged layout", () => {
  expect(slotConfig({})).toEqual({ slot: 0, display: "title", layout: "paged" });
});

test("slot index accepts the string the property inspector sends", () => {
  expect(slotConfig({ slotIndex: "3", display: "project" })).toEqual({ slot: 3, display: "project", layout: "paged" });
  expect(slotConfig({ slotIndex: 7, layout: "wall" })).toEqual({ slot: 7, display: "title", layout: "wall" });
  expect(slotConfig({ layout: "sideways" as never }).layout).toBe("paged");
});

test("garbage slot values fall back to slot 0", () => {
  expect(slotConfig({ slotIndex: "seven" }).slot).toBe(0);
  expect(slotConfig({ slotIndex: -2 }).slot).toBe(0);
  expect(slotConfig({ slotIndex: 2.9 }).slot).toBe(2);
});

test("long press threshold is the documented 400 ms and Conn sends Enter", () => {
  expect(LONG_PRESS_MS).toBe(400);
  expect(CONN_KEYS).toEqual(["enter"]);
});

test("a press acts on the session shown at key down, even if the deck reorders during the hold", async () => {
  const store = createFleetStore();
  const mk = (id: string, state: "blocked" | "idle", t: number) => {
    const [host, pane_id] = id.split("/");
    return { id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state, title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: t } as const;
  };
  store.setView({ workspaces: [], hosts: [], sessions: [mk("mini/w7:p1", "blocked", 1), mk("studio/w1:p1", "idle", 2)] });
  const sent: [string, string[]][] = [];
  let now = 1000;
  const ops = { focus: async () => false, scroll: async () => 0, prompt: async () => {}, sendKeys: async (id: string, keys: string[]) => { sent.push([id, keys]); }, focusWorkspace: async () => {} };
  const action = new InboxKeyAction(store, ops, { activate: async () => {} }, () => now);
  const alerts: string[] = [];
  const ev = (settings: object) => ({ action: { id: "k1", showAlert: async () => { alerts.push("alert"); }, showOk: async () => {} }, payload: { settings } }) as never;
  action.onKeyDown(ev({ slotIndex: "0" }));
  // Another agent blocks and sorts first while the finger is down.
  store.setView({ workspaces: [], hosts: [], sessions: [mk("studio/w1:p9", "blocked", 3), mk("mini/w7:p1", "blocked", 1), mk("studio/w1:p1", "idle", 2)] });
  now += 500;
  await action.onKeyUp(ev({ slotIndex: "0" }));
  expect(sent).toEqual([["mini/w7:p1", ["enter"]]]);
  // If the pressed session vanished, nothing is sent and the key alerts.
  action.onKeyDown(ev({ slotIndex: "1" }));
  store.setView({ workspaces: [], hosts: [], sessions: [mk("studio/w1:p1", "idle", 2)] });
  now += 500;
  await action.onKeyUp(ev({ slotIndex: "1" }));
  expect(sent).toHaveLength(1);
  expect(alerts).toEqual(["alert"]);
});

test("conn confirms only blocked sessions", async () => {
  const sent: [string, string[]][] = [];
  const ops = { focus: async () => false, scroll: async () => 0, prompt: async () => {}, sendKeys: async (id: string, keys: string[]) => { sent.push([id, keys]); }, focusWorkspace: async () => {} };
  const action = new InboxKeyAction(createFleetStore(), ops, { activate: async () => {} });
  const feedback: string[] = [];
  const ui = { showAlert: async () => { feedback.push("alert"); }, showOk: async () => { feedback.push("ok"); } };
  await action.conn({ id: "h/p1", state: "working" }, ui);
  expect(sent).toEqual([]);
  await action.conn({ id: "h/p2", state: "blocked" }, ui);
  expect(sent).toEqual([["h/p2", ["enter"]]]);
  expect(feedback).toEqual(["alert", "ok"]);
});

test("a short tap raises the terminal only when Ops did not reveal the pane", async () => {
  const store = createFleetStore();
  const session = { id: "studio/w1:p1", host: "studio", workspace_id: "w1", tab_id: "w1:t1", pane_id: "w1:p1", agent: "claude", name: null, state: "idle", title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 1 } as const;
  store.setView({ workspaces: [], hosts: [], sessions: [session] });
  const ev = { action: { id: "k1", showAlert: async () => {}, showOk: async () => {} }, payload: { settings: { slotIndex: "0" } } } as never;
  for (const revealed of [true, false]) {
    let raised = 0;
    const ops = { focus: async () => revealed, scroll: async () => 0, prompt: async () => {}, sendKeys: async () => {}, focusWorkspace: async () => {} };
    const action = new InboxKeyAction(store, ops, { activate: async () => { raised++; } }, () => 0);
    action.onKeyDown(ev);
    await action.onKeyUp(ev);
    expect(raised).toBe(revealed ? 0 : 1);
  }
});

test("action uuid matches the manifest", () => {
  expect(manifest.Actions.map((a) => a.UUID)).toContain(INBOX_KEY_UUID);
});
