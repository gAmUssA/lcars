// Wire contract with Ops. Mirrors `FleetView` in packages/ops/crates/ops/src/server.rs.

export type AgentState = "working" | "blocked" | "done" | "idle" | "unknown";

export type Pane = {
  /** `<session>/<pane_id>`, unique across the fleet. */
  id: string;
  session: string;
  workspace_id: string;
  tab_id: string;
  pane_id: string;
  /** Herdr agent kind (`claude`, `codex`, `grok`, …); null for a plain shell. */
  agent: string | null;
  name: string | null;
  state: AgentState;
  title: string | null;
  cwd: string | null;
  model: string | null;
  summary: string | null;
  focused: boolean;
  seq: number;
  updated_at_ms: number;
  /** The question a blocked agent is asking, when Ops could read it. */
  pending_prompt: string | null;
};

/** One Herdr session: one Herdr server, local or on a `machine` over SSH. */
export type SessionStatus = {
  name: string;
  /** SSH alias of the machine the session runs on; null on the deck's own machine. */
  machine: string | null;
  /** The session's Herdr runs on this machine. */
  local: boolean;
  connected: boolean;
  version: string | null;
  protocol: number | null;
  error: string | null;
  updated_at_ms: number;
};

export type Workspace = {
  /** `<session>/<workspace_id>`. */
  id: string;
  session: string;
  workspace_id: string;
  number: number;
  label: string;
  focused: boolean;
  pane_count: number;
  /** Herdr's roll-up of the agents inside. */
  state: AgentState;
};

/** `FleetView` shape this plugin reads; Ops's `API_VERSION`. */
export const API_VERSION = 2;

export type FleetView = { sessions: SessionStatus[]; panes: Pane[]; workspaces: Workspace[] };

const STATES: ReadonlySet<string> = new Set(["working", "blocked", "done", "idle", "unknown"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function str(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

/** Parse one Ops message. Throws when the shape is not a FleetView. */
export function parseFleetView(text: string): FleetView {
  const raw: unknown = JSON.parse(text);
  if (!isRecord(raw)) throw new Error("Ops message is not a FleetView (expected an object)");
  if (raw.api !== API_VERSION) {
    throw new Error(
      `Ops speaks FleetView api ${JSON.stringify(raw.api ?? 1)}, this plugin reads api ${API_VERSION}: install Ops and the plugin from the same release`,
    );
  }
  if (!Array.isArray(raw.sessions) || !Array.isArray(raw.panes)) {
    throw new Error("Ops message is not a FleetView (expected sessions[] and panes[])");
  }
  const sessions: SessionStatus[] = raw.sessions.map((h: unknown, i: number) => {
    if (!isRecord(h) || typeof h.name !== "string") throw new Error(`sessions[${i}] has no name`);
    return {
      name: h.name,
      machine: str(h.machine),
      local: h.local === true,
      connected: h.connected === true,
      version: str(h.version),
      protocol: typeof h.protocol === "number" ? h.protocol : null,
      error: str(h.error),
      updated_at_ms: typeof h.updated_at_ms === "number" ? h.updated_at_ms : 0,
    };
  });
  const panes: Pane[] = raw.panes.map((s: unknown, i: number) => {
    if (!isRecord(s) || typeof s.id !== "string" || typeof s.session !== "string" || typeof s.pane_id !== "string") {
      throw new Error(`panes[${i}] lacks id/session/pane_id`);
    }
    const state = typeof s.state === "string" && STATES.has(s.state) ? (s.state as AgentState) : "unknown";
    return {
      id: s.id,
      session: s.session,
      workspace_id: str(s.workspace_id) ?? "",
      tab_id: str(s.tab_id) ?? "",
      pane_id: s.pane_id,
      agent: str(s.agent),
      name: str(s.name),
      state,
      title: str(s.title),
      cwd: str(s.cwd),
      model: str(s.model),
      summary: str(s.summary),
      focused: s.focused === true,
      seq: typeof s.seq === "number" ? s.seq : 0,
      updated_at_ms: typeof s.updated_at_ms === "number" ? s.updated_at_ms : 0,
      pending_prompt: str(s.pending_prompt),
    };
  });
  const workspaces: Workspace[] = (Array.isArray(raw.workspaces) ? raw.workspaces : []).map((w: unknown, i: number) => {
    if (!isRecord(w) || typeof w.id !== "string" || typeof w.session !== "string" || typeof w.workspace_id !== "string") {
      throw new Error(`workspaces[${i}] lacks id/session/workspace_id`);
    }
    return {
      id: w.id,
      session: w.session,
      workspace_id: w.workspace_id,
      number: typeof w.number === "number" ? w.number : 0,
      label: str(w.label) ?? w.workspace_id,
      focused: w.focused === true,
      pane_count: typeof w.pane_count === "number" ? w.pane_count : 0,
      state: typeof w.state === "string" && STATES.has(w.state) ? (w.state as AgentState) : "unknown",
    };
  });
  return { sessions, panes, workspaces };
}
