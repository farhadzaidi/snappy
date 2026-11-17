import { defineConfig, devices } from "@playwright/test";

import { DEV_PORT as PORT } from "./devServer";

/**
 * Not tests: the README's and the store listings' screenshots of the app, against the fake
 * daemon. `tools/screenshots/capture.sh` runs this in Playwright's container.
 */
export default defineConfig({
  testDir: "screenshots",
  reporter: "list",
  use: { baseURL: `http://localhost:${PORT}` },
  // Each window is captured with the engine it runs in: WebKit on macOS, WebView2 on Windows.
  projects: [
    {
      name: "macos",
      testMatch: "app.spec.ts",
      grep: /README|Mac App Store/,
      use: { ...devices["Desktop Safari"] },
    },
    {
      name: "windows",
      testMatch: "app.spec.ts",
      grep: /Microsoft Store/,
      use: { ...devices["Desktop Chrome"] },
    },
    // Chromium encodes WebP, which WebKit can't, so the README's images are made here.
    {
      name: "readme",
      testMatch: "readme.spec.ts",
      use: { ...devices["Desktop Chrome"] },
      dependencies: ["macos", "windows"],
    },
  ],
  webServer: {
    command: `vite --port ${PORT} --strictPort`,
    port: PORT,
    reuseExistingServer: true,
  },
});
