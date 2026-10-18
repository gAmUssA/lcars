// iTerm2 automation for the remote and dictation keys. Commands are built by
// pure functions (tested); AppleScript runs through /usr/bin/osascript.
import { execFile } from "node:child_process";

const OSASCRIPT = "/usr/bin/osascript";
/** iTerm2 answers to "iTerm" in AppleScript and "iTerm2" in System Events. */
export const ITERM_APP = "iTerm";
export const ITERM_PROCESS = "iTerm2";

export type AttachMode = "herdr" | "mosh" | "tmux";
export type AttachSettings = { target?: string; mode?: AttachMode };

/** Shell command that attaches to Herdr (or tmux) on `target`; "local" or empty means this machine. */
export function attachCommand(settings: AttachSettings): string {
  const target = (settings.target ?? "").trim();
  const local = target === "" || target.toLowerCase() === "local";
  const mode: AttachMode = settings.mode === "mosh" || settings.mode === "tmux" ? settings.mode : "herdr";
  if (!/^[A-Za-z0-9._@-]*$/.test(target)) throw new Error(`attach target "${target}" is not a plain ssh alias`);
  if (local) return mode === "tmux" ? "tmux attach" : "herdr";
  switch (mode) {
    case "mosh":
      return `mosh ${target} -- herdr`;
    case "tmux":
      return `ssh -t ${target} tmux attach`;
    default:
      return `ssh -t ${target} herdr`;
  }
}

function appleScriptString(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

function shellSingleQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

/** AppleScript that opens a new iTerm2 window running `command`. */
export function openWindowScript(command: string): string {
  // Run through the user's login shell so PATH (herdr in ~/.local/bin, mosh via Homebrew) resolves.
  const shellCommand = `/bin/zsh -lc ${shellSingleQuote(command)}`;
  return [
    `tell application ${appleScriptString(ITERM_APP)}`,
    `  activate`,
    `  create window with default profile command ${appleScriptString(shellCommand)}`,
    `end tell`,
  ].join("\n");
}

/** AppleScript that clicks Edit > Start Dictation in the frontmost iTerm2 window. */
export function startDictationScript(): string {
  return [
    `tell application ${appleScriptString(ITERM_APP)} to activate`,
    `delay 0.2`,
    `tell application "System Events" to tell process ${appleScriptString(ITERM_PROCESS)}`,
    `  click (first menu item of menu "Edit" of menu bar 1 whose name starts with "Start Dictation")`,
    `end tell`,
  ].join("\n");
}

export type Osa = { run(script: string): Promise<string> };

export function createOsa(): Osa {
  return {
    run: (script) =>
      new Promise((resolve, reject) => {
        execFile(OSASCRIPT, ["-e", script], { timeout: 8000, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
          if (err) reject(new Error(`osascript failed: ${stderr || err.message}`));
          else resolve(stdout);
        });
      }),
  };
}
