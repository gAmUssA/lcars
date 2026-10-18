// Borrowed from timvdhoorn/stream-deck-herdr-plugin (MIT), see THIRD_PARTY_NOTICES.md.
export type AgentStatus = "idle" | "working" | "blocked" | "done" | "unknown";

export type Presentation = { color: string; glyph: string };

const PRESENTATION: Record<AgentStatus, Presentation> = {
  working: { color: "#E8901E", glyph: "●" },
  blocked: { color: "#D13438", glyph: "▲" },
  done: { color: "#22C55E", glyph: "✓" },
  idle: { color: "#6B7280", glyph: "○" },
  unknown: { color: "#1F2937", glyph: "·" },
};

// Off-page pager badge only fires for "needs a look" states.
// working is intentionally NOT an attention state (it is normal, expected activity).
export const ATTENTION_RANK: Record<string, number> = {
  blocked: 2,
  done: 1,
};

export function presentation(status: AgentStatus): Presentation {
  return PRESENTATION[status] ?? PRESENTATION.unknown;
}

export function isAttention(status: AgentStatus): boolean {
  return (ATTENTION_RANK[status] ?? 0) > 0;
}
