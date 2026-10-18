// Key labels for sessions. Adapted from timvdhoorn/stream-deck-herdr-plugin
// (MIT, see THIRD_PARTY_NOTICES.md): same project/title modes and duplicate
// numbering, applied to Ops sessions instead of raw Herdr agents.
import type { FleetView, Session } from "../ops/types";

export type DisplayMode = "project" | "title";

function basename(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

function truncate(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

function projectName(session: Session): string {
  return basename(session.cwd ?? "") || session.agent || "agent";
}

export function labelFor(session: Session, peers: Session[], display: DisplayMode = "title", max = 24): string {
  if (display === "title") {
    const title = (session.title ?? "").trim();
    if (title) return truncate(title, max);
    // No title: fall through to the project label.
  }
  const base = projectName(session);
  // Same project on several panes: stable #n by pane id so they read apart.
  const sameName = peers
    .filter((p) => projectName(p) === base)
    .slice()
    .sort((a, b) => a.host.localeCompare(b.host) || a.pane_id.localeCompare(b.pane_id));
  if (sameName.length <= 1) return truncate(base, max);
  const number = sameName.findIndex((p) => p.id === session.id) + 1;
  const suffix = ` #${number}`;
  return `${truncate(base, Math.max(1, max - suffix.length))}${suffix}`;
}

/** Short host tag for the key corner; null when the whole fleet is one host. */
export function hostTag(session: Session, view: FleetView | null): string | null {
  const hosts = new Set((view?.hosts ?? []).map((h) => h.name));
  for (const s of view?.sessions ?? []) hosts.add(s.host);
  if (hosts.size <= 1) return null;
  return session.host.slice(0, 4).toUpperCase();
}
