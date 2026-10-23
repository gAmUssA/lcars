import { expect, test } from "vitest";
import manifest from "../../io.gamov.lcars.sdPlugin/manifest.json";
import { createFleetStore } from "../core/store";
import { FILTER_COLOR, renderWorkspaceKey } from "../core/render";
import { agentsIn, WORKSPACE_KEY_UUID, WorkspaceKeyAction, workspaceSlot } from "./workspace-key";
import type { FleetView, Pane, Workspace } from "../ops/types";

function s(id: string, ws: string): Pane {
  const [session, pane_id] = id.split("/");
  return { id, session, workspace_id: ws, tab_id: `${ws}:t1`, pane_id, agent: "claude", name: null, state: "idle", title: null, cwd: null, model: null, summary: null, pending_prompt: null, focused: false, seq: 0, updated_at_ms: 0 };
}
const ws = (id: string, number: number, label: string, state: Workspace["state"], focused = false): Workspace => {
  const [session, workspace_id] = id.split("/");
  return { id, session, workspace_id, number, label, focused, pane_count: 2, state };
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
    sessions: [],
    panes: [s("studio/w1:p1", "w1"), s("studio/w1:p2", "w1"), s("mini/w7:p1", "w7")],
    workspaces: [ws("mini/w7", 1, "watch", "idle", true), ws("studio/w1", 1, "dotfiles", "working")],
  };
  store.setView(view);
  expect(store.workspaceAt(0)?.id).toBe("mini/w7");
  expect(store.workspaceAt(1)?.label).toBe("dotfiles");
  expect(store.workspaceAt(2)).toBeNull();
  expect(agentsIn(store, view.workspaces[1])).toBe(2);
  expect(agentsIn(store, view.workspaces[0])).toBe(1);
});

test("workspace key renders state colour, label, count, session tag and focus ring", () => {
  const uri = renderWorkspaceKey({ label: "dotfiles", state: "blocked", agents: 2, session: "STUD", focused: true, live: true });
  const svg = decodeURIComponent(uri.slice("data:image/svg+xml,".length));
  expect(svg).toContain("#D13438");
  expect(svg).toContain(">dotfiles<");
  expect(svg).toContain(">2 agents<");
  expect(svg).toContain(">STUD<");
  expect(svg).toContain('stroke="#fff"');
  const quiet = decodeURIComponent(renderWorkspaceKey({ label: "x", state: "idle", agents: 1, session: null, focused: false, live: false }).slice(19));
  expect(quiet).toContain(">1 agent<");
  expect(quiet).not.toContain('stroke="#fff"');
  expect(quiet).toContain('fill-opacity="0.6"');
  expect(renderWorkspaceKey(null)).toContain("%23000");
});

test("a press focuses the workspace and narrows the agent keys to it; a second press widens them", async () => {
  const store = createFleetStore();
  store.setView({
    sessions: [],
    panes: [s("studio/w1:p1", "w1"), s("studio/w4:p1", "w4")],
    workspaces: [ws("studio/w1", 1, "dotfiles", "idle"), ws("studio/w4", 2, "railway", "idle")],
  });
  const focused: string[] = [];
  const ops = { focus: async () => false, scroll: async () => 0, prompt: async () => {}, sendKeys: async () => {}, focusWorkspace: async (id: string) => { focused.push(id); } };
  const action = new WorkspaceKeyAction(store, ops, { activate: async () => {} });
  const ev = { action: { id: "w", showAlert: async () => {} }, payload: { settings: { slotIndex: "1" } } } as never;
  await action.onKeyDown(ev);
  expect(focused).toEqual(["studio/w4"]);
  expect(store.getState().keys.map((p) => p.id)).toEqual(["studio/w4:p1"]);
  await action.onKeyDown(ev);
  expect(store.getState().ui.workspaceFilter).toBeNull();
  expect(store.getState().keys).toHaveLength(2);
});

test("the narrowed workspace wears the filter ring", () => {
  const on = decodeURIComponent(renderWorkspaceKey({ label: "x", state: "idle", agents: 1, session: null, focused: false, filtered: true, live: true }).slice(19));
  expect(on).toContain(`stroke="${FILTER_COLOR}"`);
  const off = decodeURIComponent(renderWorkspaceKey({ label: "x", state: "idle", agents: 1, session: null, focused: false, live: true }).slice(19));
  expect(off).not.toContain(FILTER_COLOR);
});
