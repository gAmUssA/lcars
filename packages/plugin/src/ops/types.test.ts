import { expect, test } from "vitest";
import { parseFleetView } from "./types";

// Recorded from a live `GET /sessions` on Daystrom, 2026-10-18 (trimmed).
const LIVE = JSON.stringify({
  hosts: [
    { name: "daystrom", connected: true, version: "0.9.4", protocol: 22, error: null, updated_at_ms: 1760801746966 },
    { name: "ds9", connected: true, version: "0.9.0", protocol: 22, error: null, updated_at_ms: 1760801747009 },
  ],
  sessions: [
    {
      id: "daystrom/w9:p1", host: "daystrom", workspace_id: "w9", tab_id: "w9:t1", pane_id: "w9:p1",
      agent: "claude", name: null, state: "working", title: "Coding policy plugin from Tessl",
      cwd: "/Users/vikgamov/projects/lcars", model: "fable-5-1", summary: "[Request interrupted by user]",
      focused: true, seq: 4672, updated_at_ms: 1760801747080,
    },
    {
      id: "ds9/w7:p2", host: "ds9", workspace_id: "w7", tab_id: "w7:t1", pane_id: "w7:p2",
      agent: null, name: null, state: "unknown", title: null, cwd: null, model: null, summary: null,
      focused: false, seq: 0, updated_at_ms: 1760801747120,
    },
  ],
});

test("parses a live Ops view", () => {
  const view = parseFleetView(LIVE);
  expect(view.hosts.map((h) => [h.name, h.connected, h.version])).toEqual([
    ["daystrom", true, "0.9.4"],
    ["ds9", true, "0.9.0"],
  ]);
  expect(view.sessions[0].state).toBe("working");
  expect(view.sessions[0].model).toBe("fable-5-1");
  expect(view.sessions[1].agent).toBeNull();
});

test("unknown state strings become unknown, missing optionals become null", () => {
  const view = parseFleetView(
    JSON.stringify({ hosts: [], sessions: [{ id: "h/p", host: "h", pane_id: "p", state: "teleporting" }] }),
  );
  expect(view.sessions[0].state).toBe("unknown");
  expect(view.sessions[0].title).toBeNull();
  expect(view.sessions[0].seq).toBe(0);
});

test("rejects shapes that are not a FleetView", () => {
  expect(() => parseFleetView("{}")).toThrow(/FleetView/);
  expect(() => parseFleetView("[]")).toThrow(/FleetView/);
  expect(() => parseFleetView(JSON.stringify({ hosts: [{}], sessions: [] }))).toThrow(/hosts\[0\]/);
  expect(() => parseFleetView(JSON.stringify({ hosts: [], sessions: [{ id: "x" }] }))).toThrow(/sessions\[0\]/);
  expect(() => parseFleetView("not json")).toThrow();
});
