import { expect, test } from "vitest";
import { renderStripSegment, stripTap, totalsOf, type StripScene } from "./strip";
import type { Session } from "../ops/types";

function decode(uri: string): string {
  expect(uri.startsWith("data:image/svg+xml,")).toBe(true);
  return decodeURIComponent(uri.slice("data:image/svg+xml,".length));
}

function s(id: string, over: Partial<Session> = {}): Session {
  const [host, pane_id] = id.split("/");
  return {
    id, host, workspace_id: "w1", tab_id: "w1:t1", pane_id, agent: "claude", name: null, state: "idle",
    title: null, cwd: null, model: null, summary: null, focused: false, seq: 0, updated_at_ms: 0, ...over,
  };
}

const online = { connected: true, version: null, protocol: null, error: null, updated_at_ms: 0 };

const fleet: StripScene = {
  mode: "fleet",
  hosts: [
    { host: { name: "studio", ...online }, name: "studio", counts: { blocked: 1, done: 0, working: 2, idle: 3, unknown: 0 }, worst: "blocked" },
    { host: { name: "mini", ...online, connected: false }, name: "mini", counts: { blocked: 0, done: 0, working: 0, idle: 0, unknown: 0 }, worst: null },
  ],
  hostFilter: null,
  page: 0,
  pages: 2,
  effort: "high",
  blocked: [],
  tickerIndex: 0,
  totals: { blocked: 1, done: 0, working: 2, idle: 3, unknown: 0 },
  live: true,
};

test("each segment is a 200px slice of the same 800px scene", () => {
  const seg0 = decode(renderStripSegment(fleet, 0));
  const seg3 = decode(renderStripSegment(fleet, 3));
  expect(seg0).toContain('viewBox="0 0 200 100"');
  expect(seg3).toContain('viewBox="600 0 200 100"');
  // Same scene body behind both slices.
  expect(seg0.replace(/viewBox="[^"]+"/, "")).toBe(seg3.replace(/viewBox="[^"]+"/, ""));
});

test("fleet bar colours a host by its worst state and marks offline hosts", () => {
  const svg = decode(renderStripSegment(fleet, 0));
  expect(svg).toContain(">STUDIO<");
  expect(svg).toContain("#D13438"); // blocked colour
  expect(svg).toContain("1 blocked · 2 working");
  expect(svg).toContain(">MINI<");
  expect(svg).toContain(">offline<");
});

test("fleet bar fills spare segments with page and effort read-outs", () => {
  const svg = decode(renderStripSegment(fleet, 2));
  expect(svg).toContain(">PAGE<");
  expect(svg).toContain(">1/2<");
  expect(svg).toContain(">EFFORT<");
  expect(svg).toContain(">high<");
});

test("selected host gets a border and tapping it clears the filter", () => {
  const filtered = { ...fleet, hostFilter: "mini" };
  expect(decode(renderStripSegment(filtered, 1))).toContain('stroke="#fff"');
  expect(stripTap(filtered, 1, 50)).toEqual({ kind: "clear-filter" });
  expect(stripTap(filtered, 0, 50)).toEqual({ kind: "host", host: "studio" });
  expect(stripTap(filtered, 3, 50)).toEqual({ kind: "none" });
});

test("ticker shows the current blocked agent and tapping focuses it", () => {
  const blocked = [
    s("studio/w1:p2", { state: "blocked", name: "cue-judge", title: "Judge brief for notch-r2" }),
    s("mini/w7:p1", { state: "blocked" }),
  ];
  const ticker: StripScene = { ...fleet, mode: "ticker", blocked, tickerIndex: 1 };
  const svg = decode(renderStripSegment(ticker, 0));
  expect(svg).toContain(">BLOCKED<");
  expect(svg).toContain(">2 of 2<");
  expect(svg).toContain("CLAUDE on MINI");
  expect(stripTap(ticker, 2, 10)).toEqual({ kind: "focus", sessionId: "mini/w7:p1" });
  const first = decode(renderStripSegment({ ...ticker, tickerIndex: 0 }, 1));
  expect(first).toContain("CUE-JUDGE on STUDIO");
  expect(first).toContain("Judge brief for notch-r2");
});

test("ticker with nothing blocked says so and taps do nothing", () => {
  const quiet: StripScene = { ...fleet, mode: "ticker" };
  const svg = decode(renderStripSegment(quiet, 0));
  expect(svg).toContain("Nothing needs you");
  expect(svg).toContain("2 working · 3 idle");
  expect(stripTap(quiet, 0, 0)).toEqual({ kind: "none" });
});

test("a dead Ops connection paints the strip dark", () => {
  const svg = decode(renderStripSegment({ ...fleet, live: false }, 0));
  expect(svg).not.toContain("#D13438");
  expect(svg).toContain("#1a1a1a");
});

test("totals count states", () => {
  expect(totalsOf([s("h/p1", { state: "blocked" }), s("h/p2")])).toEqual({ blocked: 1, done: 0, working: 0, idle: 1, unknown: 0 });
});
