// Key presses become `POST /actions` calls to Ops, which relays them to the
// Herdr host that owns the session.

export type OpsActions = {
  focus(sessionId: string): Promise<void>;
};

export type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

export function createOpsActions(opts: { url: string; fetchImpl?: FetchLike; timeoutMs?: number }): OpsActions {
  const fetchImpl: FetchLike = opts.fetchImpl ?? ((input, init) => fetch(input, init));
  const base = opts.url.replace(/\/$/, "");
  const timeoutMs = opts.timeoutMs ?? 5000;

  const send = async (action: string, sessionId: string): Promise<void> => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetchImpl(`${base}/actions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ action, session: sessionId }),
        signal: controller.signal,
      });
      if (response.ok) return;
      let detail = `${response.status}`;
      try {
        const body = (await response.json()) as { error?: unknown };
        if (typeof body.error === "string") detail = `${response.status}: ${body.error}`;
      } catch {
        // body was not JSON; the status code is the message
      }
      throw new Error(`ops ${action} ${sessionId} failed (${detail})`);
    } finally {
      clearTimeout(timer);
    }
  };

  return {
    focus: (sessionId) => send("focus", sessionId),
  };
}
