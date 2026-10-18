import { expect, test } from "vitest";
import { createTerminalActivator, DEFAULT_TERMINAL_APP } from "./terminal";

test("activates the configured app through osascript", async () => {
  const calls: [string, string[]][] = [];
  await createTerminalActivator({ run: async (c, a) => { calls.push([c, a]); return ""; } }).activate();
  expect(calls).toEqual([["/usr/bin/osascript", ["-e", `tell application "${DEFAULT_TERMINAL_APP}" to activate`]]]);
});

test("rejects an app name that could carry AppleScript", () => {
  expect(() => createTerminalActivator({ app: 'iTerm" to do shell script "touch /tmp/x" tell application "iTerm' })).toThrow(/plain application name/);
  expect(() => createTerminalActivator({ app: "Ghostty" })).not.toThrow();
});
