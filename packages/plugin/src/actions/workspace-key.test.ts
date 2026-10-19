import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { renderWorkspaceKey } from "../core/render";
import { agentsIn, WORKSPACE_KEY_UUID, workspaceSlot } from "./workspace-key";
import type { FleetView, Session, Workspace } from "../ops/types";

function s(id: string, ws: string): Session {
  const [host, pane_id] = id.split("/");
  return { id, host, workspace_id: ws, tab_id: `${ws}:t1`, pane_id, agent: "claude", name: null, state: "idle", title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0 };
}
const ws = (id: string, number: number, label: string, state: Workspace["state"], focused = false): Workspace => {
  const [host, workspace_id] = id.split("/");
  return { id, host, workspace_id, number, label, focused, pane_count: 2, state };
};

test("the action is in the manifest and slots parse like inbox slots", () => {
  expect(manifest.Actions.map((a) => a.UUID)).toContain(WORKSPACE_KEY_UUID);
  expect(workspaceSlot({})).toBe(0);
  expect(workspaceSlot({ slotIndex: "3" })).toBe(3);
  expect(workspaceSlot({ slotIndex: -1 })).toBe(0);
});

test("workspaces come from the view in Ops order and agents are counted per workspace", () => {
  const store = createFleetStore();
  const view: FleetView = {
    hosts: [],
    sessions: [s("studio/w1:p1", "w1"), s("studio/w1:p2", "w1"), s("mini/w7:p1", "w7")],
    workspaces: [ws("mini/w7", 1, "watch", "idle", true), ws("studio/w1", 1, "dotfiles", "working")],
  };
  store.setView(view);
  expect(store.workspaceAt(0)?.id).toBe("mini/w7");
  expect(store.workspaceAt(1)?.label).toBe("dotfiles");
  expect(store.workspaceAt(2)).toBeNull();
  expect(agentsIn(store, view.workspaces[1])).toBe(2);
  expect(agentsIn(store, view.workspaces[0])).toBe(1);
});

test("workspace key renders state colour, label, count, host tag and focus ring", () => {
  const uri = renderWorkspaceKey({ label: "dotfiles", state: "blocked", agents: 2, host: "STUD", focused: true, live: true });
  const svg = decodeURIComponent(uri.slice("data:image/svg+xml,".length));
  expect(svg).toContain("#D13438");
  expect(svg).toContain(">dotfiles<");
  expect(svg).toContain(">2 agents<");
  expect(svg).toContain(">STUD<");
  expect(svg).toContain('stroke="#fff"');
  const quiet = decodeURIComponent(renderWorkspaceKey({ label: "x", state: "idle", agents: 1, host: null, focused: false, live: false }).slice(19));
  expect(quiet).toContain(">1 agent<");
  expect(quiet).not.toContain('stroke="#fff"');
  expect(quiet).toContain('fill-opacity="0.6"');
  expect(renderWorkspaceKey(null)).toContain("%23000");
});
