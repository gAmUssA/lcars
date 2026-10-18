// Key presses and dial turns become `POST /actions` calls to Ops, which
// relays them to the Herdr host that owns the session.

export type OpsActions = {
  focus(sessionId: string): Promise<void>;
  /** Scroll the pane by `delta` rows (positive = up into scrollback). Resolves to the applied offset. */
  scroll(sessionId: string, delta: number): Promise<number>;
  /** Submit `text` plus Enter to the agent in the pane. */
  prompt(sessionId: string, text: string): Promise<void>;
};

export type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

type ActionBody = { action: string; session: string; delta?: number; text?: string };

export function createOpsActions(opts: { url: string; fetchImpl?: FetchLike; timeoutMs?: number }): OpsActions {
  const fetchImpl: FetchLike = opts.fetchImpl ?? ((input, init) => fetch(input, init));
  const base = opts.url.replace(/\/$/, "");
  const timeoutMs = opts.timeoutMs ?? 5000;

  const send = async (body: ActionBody): Promise<Record<string, unknown>> => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetchImpl(`${base}/actions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
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
      throw new Error(`ops ${body.action} ${body.session} failed (${detail})`);
    } finally {
      clearTimeout(timer);
    }
  };

  return {
    focus: async (sessionId) => {
      await send({ action: "focus", session: sessionId });
    },
    scroll: async (sessionId, delta) => {
      const result = await send({ action: "scroll", session: sessionId, delta });
      return typeof result.offset_from_bottom === "number" ? result.offset_from_bottom : 0;
    },
    prompt: async (sessionId, text) => {
      await send({ action: "prompt", session: sessionId, text });
    },
  };
}
