import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { DIAL_UUIDS, ROWS_PER_TICK, sceneFrom, scrollDelta } from "./dials";
import type { FleetView, Session } from "../ops/types";

function session(id: string, over: Partial<Session> = {}): Session {
  const [host, pane_id] = id.split("/");
  return {
    id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
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

test("scene reflects the store: hosts from the full list, blocked from the filtered keys", () => {
  const store = createFleetStore({ localHost: "studio" });
  const online = { connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 };
  const view: FleetView = {
    hosts: [{ name: "studio", ...online }, { name: "mini", ...online }],
    sessions: [session("mini/w7:p1", { state: "blocked" }), session("studio/w1:p1", { state: "working" })],
  };
  store.setView(view);
  store.setHostFilter("studio");
  store.toggleStripMode();
  const scene = sceneFrom(store);
  expect(scene.mode).toBe("ticker");
  expect(scene.hosts.map((h) => h.name)).toEqual(["studio", "mini"]);
  expect(scene.hosts[1].worst).toBe("blocked"); // unfiltered summary
  expect(scene.blocked).toEqual([]); // filtered to studio, which has none
  expect(scene.totals.working).toBe(1);
  expect(scene.live).toBe(true);
});
