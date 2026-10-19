import { expect, test } from "vitest";
import { hostTag, labelFor } from "./labels";
import type { FleetView, Session } from "../ops/types";

function session(over: Partial<Session>): Session {
  return {
    id: "studio/w1:p1",
    host: "studio",
    workspace_id: "w1",
    tab_id: "w1:t1",
    pane_id: "w1:p1",
    agent: "claude",
    name: null,
    state: "idle",
    title: "Computer setup",
    cwd: "/Users/me/projects/dotfiles",
    model: null,
    summary: null,
    pending_prompt: null,
    focused: false,
    seq: 0,
    updated_at_ms: 0,
    ...over,
  };
}

test("title mode uses the pane title, truncated", () => {
  const s = session({ title: "Check Raycast window shortcuts | dotfiles" });
  expect(labelFor(s, [s], "title")).toBe("Check Raycast window sh…");
  expect(labelFor(session({ title: "Computer setup" }), [], "title")).toBe("Computer setup");
});

test("title mode falls back to the project folder when the title is empty", () => {
  const s = session({ title: "   " });
  expect(labelFor(s, [s], "title")).toBe("dotfiles");
});

test("a blocked agent's question replaces the title on its key", () => {
  const s = session({ state: "blocked", pending_prompt: "Which colour do you prefer?" });
  expect(labelFor(s, [s], "title")).toBe("Which colour do you pre…");
  expect(labelFor(s, [s], "project")).toBe("Which colour do you pre…");
  const idle = session({ state: "idle", pending_prompt: "stale" });
  expect(labelFor(idle, [idle], "title")).toBe("Computer setup");
});

test("project mode numbers panes that share a folder, across hosts", () => {
  const a = session({ id: "studio/w1:p1", pane_id: "w1:p1" });
  const b = session({ id: "studio/w1:p2", pane_id: "w1:p2" });
  const c = session({ id: "mini/w7:p1", host: "mini", pane_id: "w7:p1" });
  const peers = [c, b, a];
  // Numbered by host name then pane id: mini sorts before studio.
  expect(labelFor(c, peers, "project")).toBe("dotfiles #1");
  expect(labelFor(a, peers, "project")).toBe("dotfiles #2");
  expect(labelFor(b, peers, "project")).toBe("dotfiles #3");
});

test("project mode without a cwd falls back to the agent kind", () => {
  const s = session({ cwd: null, agent: "codex" });
  expect(labelFor(s, [s], "project")).toBe("codex");
});

test("host tag only shows for a multi-host fleet", () => {
  const one: FleetView = { workspaces: [], hosts: [{ name: "studio", local: false, connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 }], sessions: [] };
  expect(hostTag(session({}), one)).toBeNull();
  const two: FleetView = {
    workspaces: [],
    hosts: [
      ...one.hosts,
      { name: "mini", local: false, connected: false, version: null, protocol: null, error: null, updated_at_ms: 0 },
    ],
    sessions: [],
  };
  expect(hostTag(session({}), two)).toBe("STUD");
  expect(hostTag(session({ host: "mini" }), two)).toBe("MINI");
  expect(hostTag(session({}), null)).toBeNull();
});
