import { expect, test } from "vitest";
import { parseFleetView } from "./types";

// A live view recorded on Studio on 2026-10-18 (trimmed), with the keys
// renamed to api 2.
const LIVE = JSON.stringify({
  api: 2,
  sessions: [
    { name: "studio", machine: null, local: true, connected: true, version: "0.9.4", protocol: 22, error: null, updated_at_ms: 1760801746966 },
    { name: "mini", machine: "ds", local: false, connected: true, version: "0.9.0", protocol: 22, error: null, updated_at_ms: 1760801747009 },
  ],
  panes: [
    {
      id: "studio/w9:p1", session: "studio", workspace_id: "w9", tab_id: "w9:t1", pane_id: "w9:p1",
      agent: "claude", name: null, state: "working", title: "Coding policy plugin from Tessl",
      cwd: "/Users/me/projects/lcars", model: "fable-5-1", summary: "[Request interrupted by user]",
      focused: true, seq: 4672, updated_at_ms: 1760801747080,
    },
    {
      id: "mini/w7:p2", session: "mini", workspace_id: "w7", tab_id: "w7:t1", pane_id: "w7:p2",
      agent: null, name: null, state: "unknown", title: null, cwd: null, model: null, summary: null,
      focused: false, seq: 0, updated_at_ms: 1760801747120,
    },
  ],
});

test("parses a live Ops view", () => {
  const view = parseFleetView(LIVE);
  expect(view.sessions.map((h) => [h.name, h.machine, h.connected, h.version])).toEqual([
    ["studio", null, true, "0.9.4"],
    ["mini", "ds", true, "0.9.0"],
  ]);
  expect(view.panes[0].state).toBe("working");
  expect(view.panes[0].model).toBe("fable-5-1");
  expect(view.panes[1].agent).toBeNull();
  expect(view.panes[0].pending_prompt).toBeNull();
  const asking = parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [{ id: "h/p", session: "h", pane_id: "p", state: "blocked", pending_prompt: "Proceed?" }] }));
  expect(asking.panes[0].pending_prompt).toBe("Proceed?");
});

test("unknown state strings become unknown, missing optionals become null", () => {
  const view = parseFleetView(
    JSON.stringify({ api: 2, sessions: [], panes: [{ id: "h/p", session: "h", pane_id: "p", state: "teleporting" }] }),
  );
  expect(view.panes[0].state).toBe("unknown");
  expect(view.panes[0].title).toBeNull();
  expect(view.panes[0].seq).toBe(0);
});

test("workspaces parse when present and default to empty when absent", () => {
  const view = parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [], workspaces: [{ id: "studio/w1", session: "studio", workspace_id: "w1", number: 1, label: "dotfiles", focused: true, pane_count: 3, state: "working" }] }));
  expect(view.workspaces).toHaveLength(1);
  expect(view.workspaces[0].state).toBe("working");
  expect(parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [] })).workspaces).toEqual([]);
  expect(() => parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [], workspaces: [{ id: "x" }] }))).toThrow(/workspaces\[0\]/);
});

test("rejects shapes that are not a FleetView", () => {
  expect(() => parseFleetView("{}")).toThrow(/FleetView/);
  expect(() => parseFleetView("[]")).toThrow(/FleetView/);
  expect(() => parseFleetView(JSON.stringify({ api: 2, sessions: [{}], panes: [] }))).toThrow(/sessions\[0\]/);
  expect(() => parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [{ id: "x" }] }))).toThrow(/panes\[0\]/);
  expect(() => parseFleetView("not json")).toThrow();
});

test("rejects a view from an Ops that speaks another api", () => {
  // Version 1 (before Herdr naming): hosts[] plus sessions[] of panes, no api field.
  const v1 = JSON.stringify({ hosts: [{ name: "studio", connected: true }], sessions: [{ id: "studio/w1:p1", host: "studio", pane_id: "w1:p1", state: "idle" }] });
  expect(() => parseFleetView(v1)).toThrow(/api 1, this plugin reads api 2/);
  expect(() => parseFleetView(JSON.stringify({ api: 3, sessions: [], panes: [] }))).toThrow(/api 3/);
  expect(parseFleetView(JSON.stringify({ api: 2, sessions: [], panes: [] })).panes).toEqual([]);
});
