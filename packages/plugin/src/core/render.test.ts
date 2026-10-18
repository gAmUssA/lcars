import { expect, test } from "vitest";
import { BADGE_SIZE, LABEL_CHARS_PER_LINE, LABEL_FONT_PX, renderEmptySvg, renderKeySvg, renderOfflineSvg, wrapLabel } from "./render";

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

test("known agents get a centred logo, unknown agents a two-letter code", () => {
  for (const agent of ["claude", "codex", "grok"]) {
    const svg = decode(renderKeySvg({ ...base, agent }));
    expect(svg, agent).toContain(`translate(${(144 - BADGE_SIZE) / 2} 10)`);
    expect(svg, agent).not.toMatch(/>[A-Z]{2}</);
  }
  expect(BADGE_SIZE).toBe(46);
  const other = decode(renderKeySvg({ ...base, agent: "zephyr" }));
  expect(other).toContain(">ZE<");
  expect(other).toContain('x="72"');
});

test("status glyph sits in the top-left corner, title under the logo", () => {
  const svg = decode(renderKeySvg({ ...base, status: "blocked", label: "Computer setup" }));
  expect(svg).toContain('<text x="10" y="26" font-family="sans-serif" font-size="20" fill="#fff" text-anchor="start">▲</text>');
  expect(svg).toContain('y="84"');
  expect(svg).toContain('y="106"');
});

test("host tag appears only when given", () => {
  expect(decode(renderKeySvg({ ...base, host: "Mini" }))).toContain(">Mini<");
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
