import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  server: { port: 5179, strictPort: true },
  build: { target: "safari16", outDir: "dist", emptyOutDir: true },
  test: {
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/test/loadWasm.ts"],
  },
});
