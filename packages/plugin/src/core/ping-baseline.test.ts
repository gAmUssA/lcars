import { expect, test } from "vitest";
import { advance, EMPTY_BASELINE } from "./ping-baseline";
import type { FleetView, Pane } from "../ops/types";

function s(id: string, state: Pane["state"]): Pane {
  const [session, pane_id] = id.split("/");
  return { id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state, title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0 };
}
const session = (name: string, connected: boolean) => ({ name, machine: null, local: false, connected, version: null, protocol: null, error: null, updated_at_ms: 0 });
const view = (sessions: ReturnType<typeof session>[], panes: Pane[]): FleetView => ({ sessions, panes, workspaces: [] });

test("a remote session's backlog arrives after its Connected frame and does not ping", () => {
  // Frame 1: local session already up with panes; mini connected, no panes yet.
  let b = advance(EMPTY_BASELINE, view([session("studio", true), session("mini", true)], [s("studio/p1", "idle")]), [s("studio/p1", "idle")]).next;
  // Frame 2: mini's snapshot lands with an agent that was already blocked.
  const r = advance(b, view([session("studio", true), session("mini", true)], []), [s("studio/p1", "idle"), s("mini/p1", "blocked")]);
  expect(r.flips).toEqual([]);
  b = r.next;
  // Frame 3: a genuinely new block on mini pings.
  const r2 = advance(b, view([session("studio", true), session("mini", true)], []), [s("studio/p1", "idle"), s("mini/p1", "blocked"), s("mini/p2", "blocked")]);
  expect(r2.flips.map((f) => f.pane.id)).toEqual(["mini/p2"]);
});

test("a session that drops and returns is treated as a backlog again", () => {
  let b = advance(EMPTY_BASELINE, view([session("mini", true)], [s("mini/p1", "idle")]), [s("mini/p1", "idle")]).next;
  b = advance(b, view([session("mini", false)], [s("mini/p1", "idle")]), [s("mini/p1", "idle")]).next;
  b = advance(b, view([session("mini", true)], []), []).next; // Connected, snapshot pending
  const r = advance(b, view([session("mini", true)], []), [s("mini/p1", "blocked")]);
  expect(r.flips).toEqual([]);
});

test("ordinary transitions on a seeded session still ping", () => {
  const b = advance(EMPTY_BASELINE, view([session("studio", true)], []), [s("studio/p1", "working")]).next;
  const r = advance(b, view([session("studio", true)], []), [s("studio/p1", "idle")]);
  expect(r.flips.map((f) => f.kind)).toEqual(["done"]);
});
