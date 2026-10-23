// Key labels for panes. Adapted from timvdhoorn/stream-deck-herdr-plugin
// (MIT, see THIRD_PARTY_NOTICES.md): same project/title modes and duplicate
// numbering, applied to Ops panes instead of raw Herdr agents.
import type { FleetView, Pane } from "../ops/types";

export type DisplayMode = "project" | "title";

function basename(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

function projectName(pane: Pane): string {
  return basename(pane.cwd ?? "") || pane.agent || "agent";
}

export function labelFor(pane: Pane, peers: Pane[], display: DisplayMode = "title", max = 24): string {
  // A blocked agent's question beats any title: it is what the key needs answered.
  if (pane.state === "blocked" && pane.pending_prompt) return truncate(pane.pending_prompt.trim(), max);
  if (display === "title") {
    const title = (pane.title ?? "").trim();
    if (title) return truncate(title, max);
    // No title: fall through to the project label.
  }
  const base = projectName(pane);
  // Same project on several panes: stable #n by pane id so they read apart.
  const sameName = peers
    .filter((p) => projectName(p) === base)
    .slice()
    .sort((a, b) => a.session.localeCompare(b.session) || a.pane_id.localeCompare(b.pane_id));
  if (sameName.length <= 1) return truncate(base, max);
  const number = sameName.findIndex((p) => p.id === pane.id) + 1;
  const suffix = ` #${number}`;
  return `${truncate(base, Math.max(1, max - suffix.length))}${suffix}`;
}

/** Short session tag for the key corner; null when the whole fleet is one session. */
export function sessionTag(pane: Pane, view: FleetView | null): string | null {
  const sessions = new Set((view?.sessions ?? []).map((h) => h.name));
  for (const s of view?.panes ?? []) sessions.add(s.session);
  if (sessions.size <= 1) return null;
  return pane.session.slice(0, 4).toUpperCase();
}
