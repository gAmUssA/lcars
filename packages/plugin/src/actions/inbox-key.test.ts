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

test("conn confirms only blocked sessions", async () => {
  const sent: [string, string[]][] = [];
  const ops = { focus: async () => {}, scroll: async () => 0, prompt: async () => {}, sendKeys: async (id: string, keys: string[]) => { sent.push([id, keys]); } };
  const action = new InboxKeyAction(createFleetStore(), ops, { activate: async () => {} });
  const feedback: string[] = [];
  const ui = { showAlert: async () => { feedback.push("alert"); }, showOk: async () => { feedback.push("ok"); } };
  await action.conn({ id: "h/p1", state: "working" }, ui);
  expect(sent).toEqual([]);
  await action.conn({ id: "h/p2", state: "blocked" }, ui);
  expect(sent).toEqual([["h/p2", ["enter"]]]);
  expect(feedback).toEqual(["alert", "ok"]);
});

test("action uuid matches the manifest", () => {
  expect(manifest.Actions.map((a) => a.UUID)).toContain(INBOX_KEY_UUID);
});
