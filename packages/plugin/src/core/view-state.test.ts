import { expect, test } from "vitest";
import {
  blockedPanes,
  clampPage,
  cycleEffort,
  cycleSession,
  filterKeys,
  focusedPane,
  wallPageFor,
  wallSlot,
  workspacesFor,
  sessionNames,
  pageCount,
  pageSlice,
  summariseSessions,
} from "./view-state";
import type { FleetView, Pane } from "../ops/types";

function s(id: string, over: Partial<Pane> = {}): Pane {
  const [session, pane_id] = id.split("/");
  return {
    id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
  };
}

const keys = [
  s("mini/w7:p1", { state: "blocked", updated_at_ms: 50 }),
  s("studio/w1:p1", { state: "working", focused: true }),
  s("studio/w1:p2", { state: "blocked", updated_at_ms: 10 }),
  s("studio/w1:p3"),
];

const none = { sessionFilter: null, workspaceFilter: null };

test("a session filter keeps that session plus every blocked agent elsewhere", () => {
  expect(filterKeys(keys, none)).toHaveLength(4);
  // mini/w7:p1 is the only mini pane; studio's blocked pane stays pinned.
  expect(filterKeys(keys, { ...none, sessionFilter: "mini" }).map((k) => k.id)).toEqual(["mini/w7:p1", "studio/w1:p2"]);
  const calm = keys.map((k) => ({ ...k, state: "idle" as const }));
  expect(filterKeys(calm, { ...none, sessionFilter: "mini" }).map((k) => k.id)).toEqual(["mini/w7:p1"]);
});

test("a workspace filter narrows to that workspace, blocked agents still pinned", () => {
  const mixed = [s("studio/w1:p1"), s("studio/w4:p1", { workspace_id: "w4" }), s("mini/w7:p1", { workspace_id: "w7", state: "blocked" })];
  expect(filterKeys(mixed, { sessionFilter: "studio", workspaceFilter: "studio/w4" }).map((k) => k.id)).toEqual(["studio/w4:p1", "mini/w7:p1"]);
  expect(filterKeys(mixed, { sessionFilter: null, workspaceFilter: "studio/w1" }).map((k) => k.id)).toEqual(["studio/w1:p1", "mini/w7:p1"]);
});

test("workspaces follow the session filter", () => {
  const ws = (id: string) => {
    const [session, workspace_id] = id.split("/");
    return { id, session, workspace_id, number: 1, label: id, focused: false, pane_count: 1, state: "idle" as const };
  };
  const view: FleetView = { sessions: [], panes: [], workspaces: [ws("mini/w7"), ws("studio/w1"), ws("studio/w4")] };
  expect(workspacesFor(view, null).map((w) => w.id)).toEqual(["mini/w7", "studio/w1", "studio/w4"]);
  expect(workspacesFor(view, "studio").map((w) => w.id)).toEqual(["studio/w1", "studio/w4"]);
  expect(workspacesFor(null, "studio")).toEqual([]);
});

test("the wall page wraps per deck size and slots index into it", () => {
  const many = Array.from({ length: 28 }, (_, i) => s(`h/p${i}`));
  // XL wall: 16 keys, 2 pages. MK.2 wall: 6 keys, 5 pages.
  expect(wallPageFor(0, 28, 16)).toBe(0);
  expect(wallPageFor(1, 28, 16)).toBe(1);
  expect(wallPageFor(2, 28, 16)).toBe(0);
  expect(wallPageFor(4, 28, 6)).toBe(4);
  expect(wallPageFor(5, 28, 6)).toBe(0);
  expect(wallSlot(many, 1, 16, 0)?.pane_id).toBe("p16");
  expect(wallSlot(many, 1, 16, 11)?.pane_id).toBe("p27");
  expect(wallSlot(many, 1, 16, 12)).toBeNull();
  expect(wallSlot(many, 0, 0, 3)?.pane_id).toBe("p3");
});

test("paging is eight per page and clamps", () => {
  expect(pageCount(0)).toBe(1);
  expect(pageCount(8)).toBe(1);
  expect(pageCount(9)).toBe(2);
  expect(clampPage(5, 9)).toBe(1);
  expect(clampPage(-1, 9)).toBe(0);
  const many = Array.from({ length: 11 }, (_, i) => s(`h/p${i}`));
  expect(pageSlice(many, 1).map((k) => k.pane_id)).toEqual(["p8", "p9", "p10"]);
  expect(pageSlice(many, 7)).toHaveLength(3); // clamped to the last page
});

test("session cycle goes all → each session → all, both directions", () => {
  const sessions = ["studio", "mini"];
  expect(cycleSession(sessions, null, 1)).toBe("studio");
  expect(cycleSession(sessions, "studio", 1)).toBe("mini");
  expect(cycleSession(sessions, "mini", 1)).toBeNull();
  expect(cycleSession(sessions, null, -1)).toBe("mini");
  expect(cycleSession([], null, 1)).toBeNull();
  // A fast twist reports several detents at once and moves that far.
  expect(cycleSession(["studio", "mini", "laptop"], null, 3)).toBe("laptop");
  expect(cycleSession(["studio", "mini", "laptop"], null, 4)).toBeNull();
});

test("effort cycles through the levels and stops at the ends", () => {
  expect(cycleEffort("high", 1)).toBe("xhigh");
  expect(cycleEffort("max", 1)).toBe("max");
  expect(cycleEffort("low", -1)).toBe("low");
  expect(cycleEffort("medium", -3)).toBe("low");
  expect(cycleEffort("low", 3)).toBe("xhigh");
});

test("blocked panes come oldest first", () => {
  expect(blockedPanes(keys).map((k) => k.id)).toEqual(["studio/w1:p2", "mini/w7:p1"]);
});

test("focused pane prefers the deck's own session", () => {
  const both = [...keys, s("mini/w7:p9", { focused: true })];
  expect(focusedPane(both, "studio")?.id).toBe("studio/w1:p1");
  expect(focusedPane(both, "mini")?.id).toBe("mini/w7:p9");
  expect(focusedPane(both, "laptop")?.id).toBe("studio/w1:p1");
  expect(focusedPane([s("h/p1")], "h")).toBeNull();
});

test("session summaries count states and pick the worst", () => {
  const view: FleetView = {
    workspaces: [],
    sessions: [
      { name: "studio", machine: null, local: false, connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 },
      { name: "mini", machine: null, local: false, connected: false, version: null, protocol: null, error: "down", updated_at_ms: 0 },
    ],
    panes: [],
  };
  expect(sessionNames(view)).toEqual(["studio", "mini"]);
  const summary = summariseSessions(view, keys);
  expect(summary[0].worst).toBe("blocked");
  expect(summary[0].counts).toEqual({ blocked: 1, done: 0, working: 1, idle: 1, unknown: 0 });
  expect(summary[1].session?.connected).toBe(false);
  expect(summariseSessions(null, [])).toEqual([]);
});
