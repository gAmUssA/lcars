// Wire contract with Ops. Mirrors `FleetView` in packages/ops/crates/ops/src/server.rs.

export type AgentState = "working" | "blocked" | "done" | "idle" | "unknown";

export type Session = {
  /** `<host>/<pane_id>`, unique across the fleet. */
  id: string;
  host: string;
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
};

export type HostStatus = {
  name: string;
  /** The host whose Herdr runs on this machine (the deck's own host). */
  local: boolean;
  connected: boolean;
  version: string | null;
  protocol: number | null;
  error: string | null;
  updated_at_ms: number;
};

export type Workspace = {
  /** `<host>/<workspace_id>`. */
  id: string;
  host: string;
  workspace_id: string;
  number: number;
  label: string;
  focused: boolean;
  pane_count: number;
  /** Herdr's roll-up of the agents inside. */
  state: AgentState;
};

export type FleetView = { hosts: HostStatus[]; sessions: Session[]; workspaces: Workspace[] };

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
  if (!isRecord(raw) || !Array.isArray(raw.hosts) || !Array.isArray(raw.sessions)) {
    throw new Error("Ops message is not a FleetView (expected hosts[] and sessions[])");
  }
  const hosts: HostStatus[] = raw.hosts.map((h: unknown, i: number) => {
    if (!isRecord(h) || typeof h.name !== "string") throw new Error(`hosts[${i}] has no name`);
    return {
      name: h.name,
      local: h.local === true,
      connected: h.connected === true,
      version: str(h.version),
      protocol: typeof h.protocol === "number" ? h.protocol : null,
      error: str(h.error),
      updated_at_ms: typeof h.updated_at_ms === "number" ? h.updated_at_ms : 0,
    };
  });
  const sessions: Session[] = raw.sessions.map((s: unknown, i: number) => {
    if (!isRecord(s) || typeof s.id !== "string" || typeof s.host !== "string" || typeof s.pane_id !== "string") {
      throw new Error(`sessions[${i}] lacks id/host/pane_id`);
    }
    const state = typeof s.state === "string" && STATES.has(s.state) ? (s.state as AgentState) : "unknown";
    return {
      id: s.id,
      host: s.host,
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
    };
  });
  const workspaces: Workspace[] = (Array.isArray(raw.workspaces) ? raw.workspaces : []).map((w: unknown, i: number) => {
    if (!isRecord(w) || typeof w.id !== "string" || typeof w.host !== "string" || typeof w.workspace_id !== "string") {
      throw new Error(`workspaces[${i}] lacks id/host/workspace_id`);
    }
    return {
      id: w.id,
      host: w.host,
      workspace_id: w.workspace_id,
      number: typeof w.number === "number" ? w.number : 0,
      label: str(w.label) ?? w.workspace_id,
      focused: w.focused === true,
      pane_count: typeof w.pane_count === "number" ? w.pane_count : 0,
      state: typeof w.state === "string" && STATES.has(w.state) ? (w.state as AgentState) : "unknown",
    };
  });
  return { hosts, sessions, workspaces };
}
