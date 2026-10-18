import { expect, test } from "vitest";
import { ARM_WINDOW_MS, createArmedTrigger } from "./arm";

test("first press arms, second press within the window fires", () => {
  let t = 1000;
  const trigger = createArmedTrigger({ now: () => t });
  expect(trigger.isArmed()).toBe(false);
  expect(trigger.press()).toBe("armed");
  expect(trigger.isArmed()).toBe(true);
  t += 1500;
  expect(trigger.remainingMs()).toBe(ARM_WINDOW_MS - 1500);
  expect(trigger.press()).toBe("fired");
  expect(trigger.isArmed()).toBe(false);
});

test("a press after the window re-arms instead of firing", () => {
  let t = 0;
  const trigger = createArmedTrigger({ now: () => t, windowMs: 4000 });
  trigger.press();
  t = 4000;
  expect(trigger.isArmed()).toBe(false);
  expect(trigger.press()).toBe("armed");
});

test("disarm cancels", () => {
  const trigger = createArmedTrigger({ now: () => 5 });
  trigger.press();
  trigger.disarm();
  expect(trigger.isArmed()).toBe(false);
  expect(trigger.remainingMs()).toBe(0);
});
