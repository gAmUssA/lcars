import { expect, test } from "vitest";
import { renderEmptySvg, renderKeySvg, renderOfflineSvg, wrapLabel } from "./render";

function decode(uri: string): string {
  expect(uri.startsWith("data:image/svg+xml,")).toBe(true);
  return decodeURIComponent(uri.slice("data:image/svg+xml,".length));
}

const base = { label: "lcars", status: "working" as const, agent: "claude", host: null, live: true };

test("paints the status colour, glyph and escaped label", () => {
  const svg = decode(renderKeySvg({ ...base, label: "a&b", status: "blocked" }));
  expect(svg).toContain("#D13438");
  expect(svg).toContain("▲");
  expect(svg).toContain("a&amp;b");
});

test("wraps a long label onto up to three lines with an ellipsis", () => {
  expect(wrapLabel("LMManagementSystem", 8, 3)).toEqual(["LMManage", "mentSyst", "em"]);
  expect(wrapLabel("abcdefghijklmnopqrstuvwxyz", 8, 3)).toEqual(["abcdefgh", "ijklmnop", "qrstuvw…"]);
  expect(wrapLabel("short", 8, 3)).toEqual(["short"]);
});

test("known agent gets a logo badge, unknown agent a two-letter code", () => {
  const claude = decode(renderKeySvg(base));
  expect(claude).toContain("<g transform");
  const grok = decode(renderKeySvg({ ...base, agent: "grok" }));
  expect(grok).toContain(">GK<");
  const other = decode(renderKeySvg({ ...base, agent: "zephyr" }));
  expect(other).toContain(">ZE<");
});

test("host tag appears only when given", () => {
  expect(decode(renderKeySvg({ ...base, host: "DS9" }))).toContain(">DS9<");
  expect(decode(renderKeySvg(base))).not.toContain('text-anchor="end"');
});

test("a dead Ops connection dims the key but keeps its colour", () => {
  const svg = decode(renderKeySvg({ ...base, live: false }));
  expect(svg).toContain("#E8901E");
  expect(svg).toContain('fill-opacity="0.6"');
  expect(decode(renderKeySvg(base))).not.toContain('fill-opacity="0.6"');
});

test("empty slot is black and offline key says so", () => {
  expect(decode(renderKeySvg(null))).toBe(decode(renderEmptySvg()));
  expect(decode(renderEmptySvg())).toContain('fill="#000"');
  const offline = decode(renderOfflineSvg());
  expect(offline).toContain(">OPS<");
  expect(offline).toContain(">offline<");
});
