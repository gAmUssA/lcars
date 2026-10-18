import { afterEach, expect, test } from "vitest";
import { WebSocketServer, type WebSocket } from "ws";
import { createOpsClient, wsUrl, type OpsClient } from "./client";
import type { FleetView } from "./types";

const view = (n: number): FleetView => ({
  hosts: [{ name: "h", local: false, connected: true, version: "0.9.4", protocol: 22, error: null, updated_at_ms: n }],
  sessions: [],
});

let server: WebSocketServer | null = null;
let client: OpsClient | null = null;

afterEach(async () => {
  client?.stop();
  client = null;
  if (server) {
    for (const ws of server.clients) ws.terminate();
    await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = null;
  }
});

function listen(): Promise<{ url: string; sockets: WebSocket[] }> {
  return new Promise((resolve) => {
    const sockets: WebSocket[] = [];
    server = new WebSocketServer({ host: "127.0.0.1", port: 0, path: "/ws" });
    server.on("connection", (ws) => sockets.push(ws));
    server.on("listening", () => {
      const { port } = server!.address() as { port: number };
      resolve({ url: `http://127.0.0.1:${port}`, sockets });
    });
  });
}

function waitFor<T>(poll: () => T | undefined, ms = 3000): Promise<T> {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + ms;
    const tick = (): void => {
      const value = poll();
      if (value !== undefined) return resolve(value);
      if (Date.now() > deadline) return reject(new Error("timed out"));
      setTimeout(tick, 10);
    };
    tick();
  });
}

test("wsUrl maps the http base to the /ws endpoint", () => {
  expect(wsUrl("http://127.0.0.1:7677")).toBe("ws://127.0.0.1:7677/ws");
  expect(wsUrl("http://127.0.0.1:7677/")).toBe("ws://127.0.0.1:7677/ws");
  expect(wsUrl("https://ops.example:8443/base")).toBe("wss://ops.example:8443/base/ws");
});

test("delivers parsed views and ignores malformed messages", async () => {
  const { url, sockets } = await listen();
  const views: FleetView[] = [];
  const logs: string[] = [];
  client = createOpsClient({ url, onView: (v) => views.push(v), onDown: () => {}, onLog: (m) => logs.push(m) });
  client.start();
  const ws = await waitFor(() => sockets[0]);
  ws.send(JSON.stringify(view(1)));
  ws.send("{not json");
  ws.send(JSON.stringify(view(2)));
  await waitFor(() => (views.length === 2 ? true : undefined));
  expect(views.map((v) => v.hosts[0].updated_at_ms)).toEqual([1, 2]);
  expect(logs.some((l) => l.includes("malformed"))).toBe(true);
});

test("reports the loss once and reconnects", async () => {
  const { url, sockets } = await listen();
  const downs: string[] = [];
  const views: FleetView[] = [];
  client = createOpsClient({ url, onView: (v) => views.push(v), onDown: (r) => downs.push(r) });
  client.start();
  const first = await waitFor(() => sockets[0]);
  first.close(1001, "restart");
  await waitFor(() => (downs.length === 1 ? true : undefined));
  expect(downs[0]).toMatch(/closed \(1001\)/);
  // Reconnect fires after the 1 s minimum backoff.
  const second = await waitFor(() => sockets[1], 4000);
  second.send(JSON.stringify(view(9)));
  await waitFor(() => (views.length === 1 ? true : undefined));
  expect(views[0].hosts[0].updated_at_ms).toBe(9);
}, 10000);

test("stop closes the socket and suppresses further callbacks", async () => {
  const { url, sockets } = await listen();
  const downs: string[] = [];
  client = createOpsClient({ url, onView: () => {}, onDown: (r) => downs.push(r) });
  client.start();
  const ws = await waitFor(() => sockets[0]);
  client.stop();
  await waitFor(() => (ws.readyState === ws.CLOSED ? true : undefined));
  await new Promise((r) => setTimeout(r, 50));
  expect(downs).toEqual([]);
});
