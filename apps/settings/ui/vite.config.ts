import { defineConfig } from "vitest/config";

import { DEV_PORT } from "./devServer";

export default defineConfig({
  clearScreen: false,
  server: { port: DEV_PORT, strictPort: true },
  build: { target: "safari16", outDir: "dist", emptyOutDir: true },
  test: {
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test/loadWasm.ts"],
  },
});
