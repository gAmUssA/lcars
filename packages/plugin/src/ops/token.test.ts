import { expect, test } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { authHeaders, defaultTokenPath, readToken } from "./token";

test("default token path honours OPS_STATE_DIR", () => {
  expect(defaultTokenPath({ OPS_STATE_DIR: "/tmp/ops-state" })).toBe("/tmp/ops-state/token");
  expect(defaultTokenPath({})).toBe(path.join(os.homedir(), ".local", "state", "ops", "token"));
});

test("reads and trims the token, null when absent or empty", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "lcars-token-"));
  const file = path.join(dir, "token");
  expect(readToken(file)).toBeNull();
  fs.writeFileSync(file, "  abc123  \n");
  expect(readToken(file)).toBe("abc123");
  fs.writeFileSync(file, "");
  expect(readToken(file)).toBeNull();
  fs.rmSync(dir, { recursive: true });
});

test("auth headers carry the bearer token only when present", () => {
  expect(authHeaders("t")).toEqual({ authorization: "Bearer t" });
  expect(authHeaders(null)).toEqual({});
});
