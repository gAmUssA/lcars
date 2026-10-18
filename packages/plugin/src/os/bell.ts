// The done ping's sound: a macOS system sound through /usr/bin/afplay, fire
// and forget. Kept behind a tiny interface so tests and the mute setting can
// swap it out.
import { execFile } from "node:child_process";

export const DEFAULT_SOUND = "/System/Library/Sounds/Glass.aiff";
export const BLOCKED_SOUND = "/System/Library/Sounds/Ping.aiff";

export type Bell = { play(kind: "done" | "blocked"): void };

export function createBell(opts: { enabled: () => boolean; run?: (cmd: string, args: string[]) => void } = { enabled: () => true }): Bell {
  const run = opts.run ?? ((cmd, args) => execFile(cmd, args, { timeout: 5000 }, () => {}));
  return {
    play(kind) {
      if (!opts.enabled()) return;
      run("/usr/bin/afplay", [kind === "blocked" ? BLOCKED_SOUND : DEFAULT_SOUND]);
    },
  };
}
