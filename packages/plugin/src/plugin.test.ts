import { expect, test } from "vitest";
import { PLUGIN_ID } from "./plugin.js";

test("plugin id is the LCARS reverse-DNS identifier", () => {
  expect(PLUGIN_ID).toBe("io.gamov.lcars");
});
