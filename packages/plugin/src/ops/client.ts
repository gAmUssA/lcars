// WebSocket subscription to Ops. Ops pushes the full FleetView on connect and
// on every change; this client only parses and forwards. Reconnects with
// exponential backoff and never throws into the caller.
import WebSocket from "ws";
import { authHeaders } from "./token";
import { parseFleetView, type FleetView } from "./types";

export const DEFAULT_OPS_URL = "http://127.0.0.1:7677";
export const RECONNECT_MIN_MS = 1000;
export const RECONNECT_MAX_MS = 30000;

export type OpsClient = { start(): void; stop(): void };

export type OpsClientOptions = {
  /** Ops base URL, http(s)://host:port. */
  url: string;
  /** Bearer token, re-read on every connect so a restarted Ops is picked up. */
  token?: () => string | null;
  onView: (view: FleetView) => void;
  /** Called once per connection loss, with a reason for the log. */
  onDown: (reason: string) => void;
  onLog?: (message: string) => void;
};

/** `http://127.0.0.1:7677` → `ws://127.0.0.1:7677/ws`. */
export function wsUrl(base: string): string {
  const url = new URL(base);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = url.pathname.replace(/\/$/, "") + "/ws";
  url.search = "";
  return url.toString();
}

export function createOpsClient(opts: OpsClientOptions): OpsClient {
  const log = opts.onLog ?? (() => {});
  const target = wsUrl(opts.url);
  let stopped = true;
  let epoch = 0;
  let socket: WebSocket | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let backoff = RECONNECT_MIN_MS;

  const clearTimer = (): void => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  };

  const scheduleReconnect = (myEpoch: number): void => {
    if (stopped || myEpoch !== epoch) return;
    const wait = backoff;
    backoff = Math.min(backoff * 2, RECONNECT_MAX_MS);
    clearTimer();
    timer = setTimeout(() => {
      timer = null;
      connect(myEpoch);
    }, wait);
  };

  const connect = (myEpoch: number): void => {
    if (stopped || myEpoch !== epoch) return;
    log(`ops: connecting to ${target}`);
    const token = opts.token?.() ?? null;
    if (!token) log("ops: no token file yet (Ops writes <state dir>/token on start); connecting anyway");
    const ws = new WebSocket(target, { headers: authHeaders(token) });
    socket = ws;
    let wasOpen = false;
    const lost = (reason: string): void => {
      if (myEpoch !== epoch) return;
      ws.removeAllListeners();
      if (socket === ws) socket = null;
      if (wasOpen || backoff === RECONNECT_MIN_MS) opts.onDown(reason);
      scheduleReconnect(myEpoch);
    };
    ws.on("open", () => {
      wasOpen = true;
      backoff = RECONNECT_MIN_MS;
      log("ops: connected");
    });
    ws.on("message", (data) => {
      const text = typeof data === "string" ? data : data.toString();
      try {
        opts.onView(parseFleetView(text));
      } catch (e) {
        log(`ops: ignoring malformed message: ${String(e)}`);
      }
    });
    ws.on("unexpected-response", (_req, res) => {
      const why = res.statusCode === 401 ? "rejected: wrong or missing token (restart the plugin after Ops first start)" : `rejected: HTTP ${res.statusCode}`;
      lost(why);
    });
    ws.on("error", (err) => lost(`error: ${err.message}`));
    ws.on("close", (code) => lost(`closed (${code})`));
  };

  return {
    start() {
      stopped = false;
      epoch += 1;
      backoff = RECONNECT_MIN_MS;
      clearTimer();
      socket?.removeAllListeners();
      socket?.terminate();
      socket = null;
      connect(epoch);
    },
    stop() {
      stopped = true;
      epoch += 1;
      clearTimer();
      socket?.removeAllListeners();
      socket?.terminate();
      socket = null;
    },
  };
}
