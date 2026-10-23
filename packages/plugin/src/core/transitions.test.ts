import { expect, test } from "vitest";
import { detectFlips } from "./transitions";
import type { Pane } from "../ops/types";

function s(id: string, state: Pane["state"]): Pane {
  const [session, pane_id] = id.split("/");
  return {
    id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state,
    title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0,
  };
}

test("entering blocked and finishing a turn are flips; staying put is not", () => {
  const prev = [s("h/p1", "working"), s("h/p2", "idle"), s("h/p3", "blocked"), s("h/p4", "working")];
  const next = [s("h/p1", "idle"), s("h/p2", "blocked"), s("h/p3", "blocked"), s("h/p4", "working")];
  expect(detectFlips(prev, next).map((f) => [f.pane.id, f.kind])).toEqual([
    ["h/p1", "done"],
    ["h/p2", "blocked"],
  ]);
});

test("a newly seen pane pings only when blocked", () => {
  expect(detectFlips([], [s("h/p1", "idle"), s("h/p2", "blocked")]).map((f) => f.pane.id)).toEqual(["h/p2"]);
});

test("idle to idle and working to working never ping", () => {
  expect(detectFlips([s("h/p1", "idle")], [s("h/p1", "idle")])).toEqual([]);
  expect(detectFlips([s("h/p1", "working")], [s("h/p1", "working")])).toEqual([]);
});
