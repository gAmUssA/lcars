import { expect, test } from "vitest";
import { attachCommand, openWindowScript, startDictationScript } from "./iterm";

test("attach commands per mode and target", () => {
  expect(attachCommand({})).toBe("herdr");
  expect(attachCommand({ target: "local", mode: "tmux" })).toBe("tmux attach");
  expect(attachCommand({ target: "mini" })).toBe("ssh -t mini herdr");
  expect(attachCommand({ target: "mini", mode: "mosh" })).toBe("mosh mini -- herdr");
  expect(attachCommand({ target: "mini", mode: "tmux" })).toBe("ssh -t mini tmux attach");
  expect(attachCommand({ target: "me@studio.example", mode: "herdr" })).toBe("ssh -t me@studio.example herdr");
});

test("attach refuses targets that are not plain aliases", () => {
  expect(() => attachCommand({ target: "mini; rm -rf /" })).toThrow(/plain ssh alias/);
  expect(() => attachCommand({ target: "a b" })).toThrow();
});

test("window script targets iTerm and runs the command through a login shell", () => {
  const script = openWindowScript("ssh -t mini herdr");
  expect(script).toContain('tell application "iTerm"');
  expect(script).toContain("create window with default profile command");
  expect(script).toContain(`command "/bin/zsh -lc 'ssh -t mini herdr'"`);
});

test("dictation script clicks the Start Dictation menu item in iTerm2", () => {
  const script = startDictationScript();
  expect(script).toContain('process "iTerm2"');
  expect(script).toContain('whose name starts with "Start Dictation"');
});
