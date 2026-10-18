// Key images as inline SVG data URIs. Adapted from
// timvdhoorn/stream-deck-herdr-plugin (MIT, see THIRD_PARTY_NOTICES.md):
// status colour + glyph, agent badge, wrapped label. LCARS adds the host tag
// and the dimmed "Ops offline" treatment; the pin marker is gone.
import { presentation, type AgentStatus } from "./status";
import { AGENT_ICON } from "./agent-icons";

export type KeyView = {
  label: string;
  status: AgentStatus;
  agent: string;
  /** Short host tag for a multi-host fleet, or null. */
  host: string | null;
  /** False when Ops is unreachable: the key keeps its last state but dims. */
  live: boolean;
};

function toDataUri(svg: string): string {
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export function escapeXml(value: string): string {
  const map: Record<string, string> = { "<": "&lt;", ">": "&gt;", "&": "&amp;", "'": "&apos;", '"': "&quot;" };
  return value.replace(/[<>&'"]/g, (c) => map[c] ?? c);
}

// Wrap a label into up to maxLines lines of at most perLine chars, breaking
// at spaces when a word fits and inside a word only when it is longer than a
// line. The last line is ellipsised if text remains.
export function wrapLabel(label: string, perLine: number, maxLines: number): string[] {
  const words = label.trim().split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";
  let overflow = false;
  const flush = (): boolean => {
    if (lines.length === maxLines) return false;
    lines.push(current);
    current = "";
    return true;
  };
  for (let i = 0; i < words.length && !overflow; i++) {
    let word = words[i];
    if (current && current.length + 1 + word.length <= perLine) {
      current = `${current} ${word}`;
      continue;
    }
    if (current && !flush()) {
      overflow = true;
      break;
    }
    while (word.length > perLine) {
      current = word.slice(0, perLine);
      word = word.slice(perLine);
      if (lines.length === maxLines - 1) {
        overflow = true;
        break;
      }
      flush();
    }
    if (overflow) {
      current = `${current.slice(0, perLine - 1)}…`;
      lines.push(current);
      return lines;
    }
    current = word;
    if (i === words.length - 1) flush();
    else if (lines.length === maxLines) overflow = true;
  }
  if (overflow || (current && lines.length === maxLines)) {
    const last = lines.length === maxLines ? lines.pop()! : current;
    const cut = last.length >= perLine ? last.slice(0, perLine - 1) : last;
    lines.push(`${cut}…`);
    return lines;
  }
  if (current) lines.push(current);
  return lines.length ? lines : [""];
}

// Short 2-letter code per known herdr agent integration; falls back to the
// first two letters of any other agent name.
const AGENT_CODE: Record<string, string> = {
  claude: "CL", codex: "CX", copilot: "CP", cursor: "CU", devin: "DV", droid: "DR", grok: "GK",
  kimi: "KM", opencode: "OC", kilo: "KL", hermes: "HM", qodercli: "QC", pi: "PI", omp: "OM",
};

function agentCode(name: string): string {
  return AGENT_CODE[name.toLowerCase()] ?? name.slice(0, 2).toUpperCase();
}

const BADGE_X = 8;
const BADGE_Y = 8;
const BADGE_SIZE = 30;

// Agent-type badge in the top-left corner: a monochrome logo when one exists,
// otherwise the 2-letter code.
function agentBadge(name: string): string {
  const icon = AGENT_ICON[name.toLowerCase()];
  if (icon) {
    const [minX, minY, w, h] = icon.vb.split(/\s+/).map(Number);
    const scale = BADGE_SIZE / Math.max(w, h);
    const transform = `translate(${BADGE_X} ${BADGE_Y}) scale(${scale}) translate(${-minX} ${-minY})`;
    return `<g transform="${transform}" fill="#ffffff" fill-opacity="0.85" fill-rule="evenodd">${icon.body}</g>`;
  }
  return (
    `<text x="10" y="28" font-family="sans-serif" font-size="18" font-weight="bold" ` +
    `fill="#ffffff" fill-opacity="0.75" text-anchor="start">${escapeXml(agentCode(name))}</text>`
  );
}

export function renderEmptySvg(): string {
  return toDataUri(
    `<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144"><rect width="144" height="144" rx="16" fill="#000"/></svg>`,
  );
}

/** Shown on every key while Ops has never connected. */
export function renderOfflineSvg(): string {
  return toDataUri(
    `<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144">` +
      `<rect width="144" height="144" rx="16" fill="#0a0a0a"/>` +
      `<text x="72" y="66" font-family="sans-serif" font-size="26" fill="#444" text-anchor="middle">OPS</text>` +
      `<text x="72" y="96" font-family="sans-serif" font-size="18" fill="#444" text-anchor="middle">offline</text>` +
      `</svg>`,
  );
}

/** Label typography: three lines of up to 11 characters at 19 px, natural width. */
export const LABEL_CHARS_PER_LINE = 11;
export const LABEL_LINES = 3;
export const LABEL_FONT_PX = 19;
const LABEL_LINE_HEIGHT = 22;
const LABEL_LAST_BASELINE = 130;

export function renderKeySvg(view: KeyView | null): string {
  if (!view) return renderEmptySvg();
  const { color, glyph } = presentation(view.status);
  const lines = wrapLabel(view.label, LABEL_CHARS_PER_LINE, LABEL_LINES);
  const firstY = LABEL_LAST_BASELINE - (lines.length - 1) * LABEL_LINE_HEIGHT;
  const labelSvg = lines
    .map((line, i) => {
      const y = firstY + i * LABEL_LINE_HEIGHT;
      return `<text x="72" y="${y}" font-family="sans-serif" font-size="${LABEL_FONT_PX}" fill="#fff" text-anchor="middle">${escapeXml(line)}</text>`;
    })
    .join("");
  const host = view.host
    ? `<text x="136" y="24" font-family="sans-serif" font-size="14" font-weight="bold" fill="#ffffff" fill-opacity="0.8" text-anchor="end">${escapeXml(view.host)}</text>`
    : "";
  // Dim the whole key when Ops is gone so stale state reads as stale.
  const dim = view.live ? "" : `<rect width="144" height="144" rx="16" fill="#000" fill-opacity="0.6"/>`;
  return toDataUri(
    `<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144">` +
      `<rect width="144" height="144" rx="16" fill="${color}"/>` +
      `<text x="72" y="34" font-family="sans-serif" font-size="24" fill="#fff" text-anchor="middle">${glyph}</text>` +
      agentBadge(view.agent) +
      host +
      labelSvg +
      dim +
      `</svg>`,
  );
}
