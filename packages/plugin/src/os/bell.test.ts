import { expect, test } from "vitest";
import { BLOCKED_SOUND, createBell, DEFAULT_SOUND } from "./bell";

test("plays the matching system sound through afplay when enabled", () => {
  const calls: [string, string[]][] = [];
  const bell = createBell({ enabled: () => true, run: (c, a) => calls.push([c, a]) });
  bell.play("done");
  bell.play("blocked");
  expect(calls).toEqual([
    ["/usr/bin/afplay", [DEFAULT_SOUND]],
    ["/usr/bin/afplay", [BLOCKED_SOUND]],
  ]);
});

test("stays silent when muted", () => {
  const calls: string[] = [];
  createBell({ enabled: () => false, run: (c) => calls.push(c) }).play("done");
  expect(calls).toEqual([]);
});
