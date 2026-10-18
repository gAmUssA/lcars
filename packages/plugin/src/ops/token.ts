// Ops writes a bearer token to <state dir>/token (mode 0600). The plugin runs
// as the same user and reads it; nothing else on the machine can.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

export function defaultTokenPath(env: NodeJS.ProcessEnv = process.env): string {
  const dir = env.OPS_STATE_DIR && env.OPS_STATE_DIR.trim() !== "" ? env.OPS_STATE_DIR : path.join(os.homedir(), ".local", "state", "ops");
  return path.join(dir, "token");
}

/** Token text, or null when the file is missing (Ops not installed or not started yet). */
export function readToken(file: string = defaultTokenPath()): string | null {
  try {
    const text = fs.readFileSync(file, "utf8").trim();
    return text.length > 0 ? text : null;
  } catch {
    return null;
  }
}

export function authHeaders(token: string | null): Record<string, string> {
  return token ? { authorization: `Bearer ${token}` } : {};
}
