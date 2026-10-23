// Touch strip rendering for the Stream Deck +. The strip is four 200×100
// segments, one per dial action. We draw one 800×100 scene and hand each
// dial the slice under it, so the strip reads as a single surface.
import { presentation } from "./status";
import { escapeXml } from "./render";
import type { SessionStatus, Pane } from "../ops/types";
import type { Effort, SessionSummary, StripMode } from "./view-state";

export const STRIP_WIDTH = 800;
export const SEGMENT_WIDTH = 200;
export const STRIP_HEIGHT = 100;
export const SEGMENTS = 4;

export type StripScene = {
  mode: StripMode;
  sessions: SessionSummary[];
  sessionFilter: string | null;
  /** Label of the workspace the keys are narrowed to, if any. */
  workspaceLabel?: string | null;
  page: number;
  pages: number;
  effort: Effort;
  /** Blocked panes in ticker order and the cursor into them. */
  blocked: Pane[];
  tickerIndex: number;
  /** Counts across the visible keys, for the "all quiet" ticker. */
  totals: Record<Pane["state"], number>;
  live: boolean;
};

export type StripTap =
  | { kind: "session"; session: string }
  | { kind: "clear-filter" }
  | { kind: "focus"; paneId: string }
  | { kind: "none" };

const FONT = `font-family="sans-serif"`;

function text(x: number, y: number, size: number, body: string, extra = ""): string {
  return `<text x="${x}" y="${y}" ${FONT} font-size="${size}" fill="#fff" ${extra}>${escapeXml(body)}</text>`;
}

function dim(color: string, live: boolean): string {
  return live ? color : "#1a1a1a";
}

function sessionBlock(i: number, summary: SessionSummary, scene: StripScene): string {
  const x = i * SEGMENT_WIDTH;
  const online = summary.session?.connected ?? true;
  const color = !online ? "#3a3a3a" : summary.worst ? presentation(summary.worst).color : "#374151";
  const selected = scene.sessionFilter === summary.name;
  const border = selected ? `<rect x="${x + 3}" y="3" width="${SEGMENT_WIDTH - 6}" height="${STRIP_HEIGHT - 6}" rx="10" fill="none" stroke="#fff" stroke-width="4"/>` : "";
  const c = summary.counts;
  const detail = !online
    ? "offline"
    : [c.blocked && `${c.blocked} blocked`, c.working && `${c.working} working`, c.done && `${c.done} done`, c.idle && `${c.idle} idle`]
        .filter(Boolean)
        .slice(0, 2)
        .join(" · ") || "no agents";
  return (
    `<rect x="${x}" y="0" width="${SEGMENT_WIDTH}" height="${STRIP_HEIGHT}" fill="${dim(color, scene.live)}"/>` +
    border +
    text(x + 16, 42, 30, summary.name.toUpperCase(), `font-weight="bold"`) +
    text(x + 16, 76, 20, detail)
  );
}

function labelBlock(i: number, title: string, value: string, scene: StripScene): string {
  const x = i * SEGMENT_WIDTH;
  return (
    `<rect x="${x}" y="0" width="${SEGMENT_WIDTH}" height="${STRIP_HEIGHT}" fill="${dim("#111827", scene.live)}"/>` +
    text(x + 16, 36, 18, title, `fill-opacity="0.7"`) +
    text(x + 16, 76, 30, value, `font-weight="bold"`)
  );
}

function fleetScene(scene: StripScene): string {
  const blocks: string[] = [];
  const sessions = scene.sessions.slice(0, SEGMENTS);
  sessions.forEach((h, i) => blocks.push(sessionBlock(i, h, scene)));
  // Remaining segments carry the dial read-outs that have no session to show.
  const labels: [string, string][] = [
    ["PAGE", `${scene.page + 1}/${scene.pages}`],
    ["EFFORT", scene.effort],
    ["FILTER", scene.workspaceLabel ?? scene.sessionFilter ?? "all"],
  ];
  for (let i = sessions.length, l = 0; i < SEGMENTS && l < labels.length; i++, l++) {
    blocks.push(labelBlock(i, labels[l][0], labels[l][1], scene));
  }
  return blocks.join("");
}

function tickerScene(scene: StripScene): string {
  const current = scene.blocked[scene.tickerIndex];
  if (!current) {
    const t = scene.totals;
    const summary = [t.working && `${t.working} working`, t.done && `${t.done} done`, t.idle && `${t.idle} idle`].filter(Boolean).join(" · ");
    return (
      `<rect x="0" y="0" width="${STRIP_WIDTH}" height="${STRIP_HEIGHT}" fill="${dim("#0f172a", scene.live)}"/>` +
      text(24, 44, 30, "Nothing needs you", `font-weight="bold"`) +
      text(24, 80, 22, summary || "no agents", `fill-opacity="0.75"`)
    );
  }
  const red = presentation("blocked").color;
  const title = current.pending_prompt ?? current.title ?? current.name ?? current.agent ?? current.pane_id;
  const who = `${(current.name ?? current.agent ?? "agent").toUpperCase()} on ${current.session.toUpperCase()}`;
  return (
    `<rect x="0" y="0" width="${SEGMENT_WIDTH}" height="${STRIP_HEIGHT}" fill="${dim(red, scene.live)}"/>` +
    text(16, 42, 30, "BLOCKED", `font-weight="bold"`) +
    text(16, 78, 24, `${scene.tickerIndex + 1} of ${scene.blocked.length}`) +
    `<rect x="${SEGMENT_WIDTH}" y="0" width="${STRIP_WIDTH - SEGMENT_WIDTH}" height="${STRIP_HEIGHT}" fill="${dim("#1f2937", scene.live)}"/>` +
    text(SEGMENT_WIDTH + 16, 36, 20, who, `fill-opacity="0.75"`) +
    text(SEGMENT_WIDTH + 16, 76, 30, truncate(title, 36), `font-weight="bold"`)
  );
}

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

/** The whole 800×100 scene body (no outer <svg>). */
export function sceneBody(scene: StripScene): string {
  return scene.mode === "fleet" ? fleetScene(scene) : tickerScene(scene);
}

/** SVG data URI for the slice of the scene under dial `segment` (0–3). */
export function renderStripSegment(scene: StripScene, segment: number): string {
  const x = Math.min(Math.max(0, segment), SEGMENTS - 1) * SEGMENT_WIDTH;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${SEGMENT_WIDTH}" height="${STRIP_HEIGHT}" viewBox="${x} 0 ${SEGMENT_WIDTH} ${STRIP_HEIGHT}">` +
    sceneBody(scene) +
    `</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** What a tap at `x` px inside `segment` means. */
export function stripTap(scene: StripScene, segment: number, x: number): StripTap {
  if (scene.mode === "ticker") {
    const current = scene.blocked[scene.tickerIndex];
    return current ? { kind: "focus", paneId: current.id } : { kind: "none" };
  }
  const session = scene.sessions[segment];
  if (!session) return { kind: "none" };
  void x;
  return scene.sessionFilter === session.name ? { kind: "clear-filter" } : { kind: "session", session: session.name };
}

export function emptyTotals(): Record<Pane["state"], number> {
  return { blocked: 0, done: 0, working: 0, idle: 0, unknown: 0 };
}

export function totalsOf(keys: Pane[]): Record<Pane["state"], number> {
  const t = emptyTotals();
  for (const s of keys) t[s.state] += 1;
  return t;
}

export type { SessionStatus };
