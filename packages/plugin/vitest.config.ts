import { defineConfig } from "vitest/config";

// The Stream Deck SDK opens and rotates ./logs on import; suites that import
// an action class must not race each other on that directory.
export default defineConfig({
  test: { fileParallelism: false },
});
