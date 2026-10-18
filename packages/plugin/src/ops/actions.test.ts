import { afterEach, expect, test } from "vitest";
import http from "node:http";
import { createOpsActions } from "./actions";

type Seen = { method: string | undefined; url: string | undefined; body: string };

let server: http.Server | null = null;

afterEach(async () => {
  if (server) {
    await new Promise<void>((resolve) => server!.close(() => resolve()));
    server = null;
  }
});

function serve(status: number, reply: string): Promise<{ url: string; seen: Seen[] }> {
  const seen: Seen[] = [];
  return new Promise((resolve) => {
    server = http.createServer((req, res) => {
      let body = "";
      req.on("data", (c: Buffer) => (body += c.toString()));
      req.on("end", () => {
        seen.push({ method: req.method, url: req.url, body });
        res.writeHead(status, { "content-type": "application/json" });
        res.end(reply);
      });
    });
    server.listen(0, "127.0.0.1", () => {
      const { port } = server!.address() as { port: number };
      resolve({ url: `http://127.0.0.1:${port}`, seen });
    });
  });
}

test("focus posts the session id to /actions", async () => {
  const { url, seen } = await serve(200, '{"ok":true}');
  await createOpsActions({ url }).focus("mini/w7:p1");
  expect(seen).toHaveLength(1);
  expect(seen[0].method).toBe("POST");
  expect(seen[0].url).toBe("/actions");
  expect(JSON.parse(seen[0].body)).toEqual({ action: "focus", session: "mini/w7:p1" });
});

test("a failure surfaces the status and Ops's error text", async () => {
  const { url } = await serve(502, '{"ok":false,"error":"no SSH master for mini"}');
  await expect(createOpsActions({ url }).focus("mini/w7:p1")).rejects.toThrow(/502: no SSH master for mini/);
});

test("a non-JSON failure still reports the status", async () => {
  const { url } = await serve(404, "nope");
  await expect(createOpsActions({ url }).focus("x/y")).rejects.toThrow(/failed \(404\)/);
});

test("scroll posts the delta and returns the applied offset", async () => {
  const { url, seen } = await serve(200, '{"ok":true,"offset_from_bottom":13}');
  const offset = await createOpsActions({ url }).scroll("mini/w7:p1", 3);
  expect(offset).toBe(13);
  expect(JSON.parse(seen[0].body)).toEqual({ action: "scroll", session: "mini/w7:p1", delta: 3 });
});

test("prompt posts the text", async () => {
  const { url, seen } = await serve(200, '{"ok":true}');
  await createOpsActions({ url }).prompt("studio/w1:p1", "/effort high");
  expect(JSON.parse(seen[0].body)).toEqual({ action: "prompt", session: "studio/w1:p1", text: "/effort high" });
});
