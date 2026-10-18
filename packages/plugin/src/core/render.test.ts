import { expect, test } from "vitest";
import { LABEL_CHARS_PER_LINE, LABEL_FONT_PX, renderEmptySvg, renderKeySvg, renderOfflineSvg, wrapLabel } from "./render";

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

test("wraps at word boundaries, splits only over-long words, ellipsises overflow", () => {
  expect(wrapLabel("Coding policy plugin from Tessl", 11, 3)).toEqual(["Coding", "policy", "plugin fro…"]);
  expect(wrapLabel("Computer setup", 11, 3)).toEqual(["Computer", "setup"]);
  expect(wrapLabel("claude-watch local setup", 11, 3)).toEqual(["claude-watc", "h local", "setup"]);
  expect(wrapLabel("LMManagementSystem", 8, 3)).toEqual(["LMManage", "mentSyst", "em"]);
  expect(wrapLabel("abcdefghijklmnopqrstuvwxyz", 8, 3)).toEqual(["abcdefgh", "ijklmnop", "qrstuvw…"]);
  expect(wrapLabel("short", 8, 3)).toEqual(["short"]);
  expect(wrapLabel("a b c d e f g", 3, 2)).toEqual(["a b", "c …"]);
  expect(wrapLabel("", 8, 3)).toEqual([""]);
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

test("labels are 19px, up to 11 characters per line, never stretched", () => {
  expect(LABEL_FONT_PX).toBe(19);
  expect(LABEL_CHARS_PER_LINE).toBe(11);
  const svg = decode(renderKeySvg({ ...base, label: "Computer setup" }));
  expect(svg).toContain('font-size="19"');
  expect(svg).not.toContain("textLength");
  expect(svg).toContain(">Computer<");
  expect(svg).toContain(">setup<");
});
