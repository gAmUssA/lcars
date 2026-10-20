// Key presses and dial turns become `POST /actions` calls to Ops, which
// relays them to the Herdr host that owns the session.

import { authHeaders } from "./token";

export type OpsActions = {
  /** Focus the pane. Resolves true when Ops also revealed its iTerm session; otherwise raise the terminal yourself. */
  focus(sessionId: string): Promise<boolean>;
  /** Scroll the pane by `delta` rows (positive = up into scrollback). Resolves to the applied offset. */
  scroll(sessionId: string, delta: number): Promise<number>;
  /** Submit `text` plus Enter to the agent in the pane. */
  prompt(sessionId: string, text: string): Promise<void>;
  /** Press Herdr logical keys (`enter`, `esc`, …) in the pane. */
  sendKeys(sessionId: string, keys: string[]): Promise<void>;
  /** Switch a host's Herdr to the workspace (`host/w1`). */
  focusWorkspace(workspaceId: string): Promise<void>;
};

export type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

type ActionBody = { action: string; session?: string; workspace?: string; delta?: number; text?: string; keys?: string[] };

export function createOpsActions(opts: { url: string; token?: () => string | null; fetchImpl?: FetchLike; timeoutMs?: number }): OpsActions {
  const fetchImpl: FetchLike = opts.fetchImpl ?? ((input, init) => fetch(input, init));
  const base = opts.url.replace(/\/$/, "");
  const timeoutMs = opts.timeoutMs ?? 5000;

  const send = async (body: ActionBody): Promise<Record<string, unknown>> => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetchImpl(`${base}/actions`, {
        method: "POST",
        headers: { "content-type": "application/json", ...authHeaders(opts.token?.() ?? null) },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      let parsed: Record<string, unknown> = {};
      try {
        parsed = (await response.json()) as Record<string, unknown>;
      } catch {
        // body was not JSON; the status code is the message
      }
      if (response.ok) return parsed;
      const detail = typeof parsed.error === "string" ? `${response.status}: ${parsed.error}` : `${response.status}`;
      throw new Error(`ops ${body.action} ${body.session ?? body.workspace} failed (${detail})`);
    } finally {
      clearTimeout(timer);
    }
  };

  return {
    focus: async (sessionId) => {
      const result = await send({ action: "focus", session: sessionId });
      return result.revealed === true;
    },
    scroll: async (sessionId, delta) => {
      const result = await send({ action: "scroll", session: sessionId, delta });
      return typeof result.offset_from_bottom === "number" ? result.offset_from_bottom : 0;
    },
    prompt: async (sessionId, text) => {
      await send({ action: "prompt", session: sessionId, text });
    },
    sendKeys: async (sessionId, keys) => {
      await send({ action: "send_keys", session: sessionId, keys });
    },
    focusWorkspace: async (workspaceId) => {
      await send({ action: "focus_workspace", workspace: workspaceId });
    },
  };
}
