// Which panes just flipped into a state worth a ping. Adapted from
// timvdhoorn/stream-deck-herdr-plugin (MIT, see THIRD_PARTY_NOTICES.md).
import type { Pane } from "../ops/types";

export type Flip = { pane: Pane; kind: "blocked" | "done" };

/**
 * Panes that entered `blocked`, or finished a turn (working → idle/done),
 * between two views. A pane unseen before counts only if it is blocked:
 * a freshly discovered idle agent did not just finish anything.
 */
export function detectFlips(prev: Pane[], next: Pane[]): Flip[] {
  const before = new Map(prev.map((s) => [s.id, s.state]));
  const flips: Flip[] = [];
  for (const s of next) {
    const was = before.get(s.id);
    if (s.state === "blocked" && was !== "blocked") flips.push({ pane: s, kind: "blocked" });
    else if ((s.state === "idle" || s.state === "done") && was === "working") flips.push({ pane: s, kind: "done" });
  }
  return flips;
}
