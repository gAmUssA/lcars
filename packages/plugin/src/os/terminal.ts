// Borrowed from timvdhoorn/stream-deck-herdr-plugin (MIT), see THIRD_PARTY_NOTICES.md.
import { execFile } from "node:child_process";

export type RunFn = (cmd: string, args: string[]) => Promise<string>;

// Absolute path so the call survives Stream Deck's minimal launch PATH (osascript
// is a fixed macOS system binary).
const OSASCRIPT = "/usr/bin/osascript";

// AppleScript application name the host terminal answers to. iTerm2 responds to
// "iTerm". Override via LCARS_TERMINAL_APP when herdr runs in another
// terminal (e.g. "Terminal", "Ghostty", "WezTerm").
export const DEFAULT_TERMINAL_APP = "iTerm";

const defaultRun: RunFn = (cmd, args) =>
  new Promise((resolve, reject) => {
    execFile(cmd, args, { timeout: 4000, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) reject(new Error(`${cmd} ${args.join(" ")} failed: ${stderr || err.message}`));
      else resolve(stdout);
    });
  });

export type TerminalActivator = { activate(): Promise<void> };

// Bring the host terminal app to the foreground. `activate` is idempotent: if the
// app is already frontmost it is a no-op, so this can be called on every focus
// without a separate "is it focused?" check. Focusing a herdr pane only switches
// the pane *inside* herdr; raising the GUI app is what puts it on screen when the
// terminal was in the background.
export function createTerminalActivator(opts: { run?: RunFn; app?: string } = {}): TerminalActivator {
  const run = opts.run ?? defaultRun;
  const app = opts.app ?? DEFAULT_TERMINAL_APP;
  // The name comes from an environment variable: keep it a plain app name so
  // it cannot smuggle AppleScript into the `tell` line.
  if (!/^[A-Za-z0-9 ._-]{1,64}$/.test(app)) {
    throw new Error(`LCARS_TERMINAL_APP must be a plain application name, got ${JSON.stringify(app)}`);
  }
  return {
    async activate() {
      await run(OSASCRIPT, ["-e", `tell application "${app}" to activate`]);
    },
  };
}
