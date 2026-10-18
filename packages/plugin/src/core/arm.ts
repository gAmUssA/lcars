// Two-press confirmation: the first press arms for a short window, the
// second press inside the window fires. Pure, clock injected.

export const ARM_WINDOW_MS = 4000;

export type ArmedTrigger = {
  /** Returns "armed" on the first press and "fired" on a confirming press. */
  press(): "armed" | "fired";
  /** True while a press would fire. */
  isArmed(): boolean;
  /** Milliseconds left in the window, 0 when disarmed. */
  remainingMs(): number;
  disarm(): void;
};

export function createArmedTrigger(opts: { now?: () => number; windowMs?: number } = {}): ArmedTrigger {
  const now = opts.now ?? Date.now;
  const windowMs = opts.windowMs ?? ARM_WINDOW_MS;
  let armedAt: number | null = null;
  const armed = (): boolean => armedAt !== null && now() - armedAt < windowMs;
  return {
    press() {
      if (armed()) {
        armedAt = null;
        return "fired";
      }
      armedAt = now();
      return "armed";
    },
    isArmed: armed,
    remainingMs: () => (armed() ? windowMs - (now() - (armedAt as number)) : 0),
    disarm() {
      armedAt = null;
    },
  };
}
