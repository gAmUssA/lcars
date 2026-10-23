import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { DIAL_UUIDS, ROWS_PER_TICK, sceneFrom, scrollDelta } from "./dials";
import type { FleetView, Pane } from "../ops/types";

function pane(id: string, over: Partial<Pane> = {}): Pane {
  const [session, pane_id] = id.split("/");
  return {
    id, session, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
  };
}

test("every dial uuid is declared in the manifest as an Encoder with the strip layout", () => {
  const encoders = manifest.Actions.filter((a) => a.Controllers.includes("Encoder"));
  expect(encoders.map((a) => a.UUID).sort()).toEqual(Object.values(DIAL_UUIDS).sort());
  for (const a of encoders) expect((a as { Encoder?: { layout?: string } }).Encoder?.layout).toBe("layouts/strip.json");
});

test("clockwise ticks scroll toward the bottom, three rows per tick", () => {
  expect(ROWS_PER_TICK).toBe(3);
  expect(scrollDelta(2)).toBe(-6);
  expect(scrollDelta(-1)).toBe(3);
});

test("scene reflects the store: sessions from the full list, blocked from the filtered keys", () => {
  const store = createFleetStore({ localSession: "studio" });
  const online = { machine: null, local: false, connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 };
  const view: FleetView = {
    workspaces: [],
    sessions: [{ name: "studio", ...online }, { name: "mini", ...online }],
    panes: [pane("mini/w7:p1", { state: "blocked" }), pane("studio/w1:p1", { state: "working" })],
  };
  store.setView(view);
  store.setSessionFilter("studio");
  store.toggleStripMode();
  const scene = sceneFrom(store);
  expect(scene.mode).toBe("ticker");
  expect(scene.sessions.map((h) => h.name)).toEqual(["studio", "mini"]);
  expect(scene.sessions[1].worst).toBe("blocked"); // unfiltered summary
  expect(scene.blocked).toEqual([]); // filtered to studio, which has none
  expect(scene.totals.working).toBe(1);
  expect(scene.live).toBe(true);
});
