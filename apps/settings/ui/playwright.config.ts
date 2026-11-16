import { defineConfig, devices } from "@playwright/test";

/** `SNAPPY_UI_PORT` gives a run a server of its own, apart from another checkout's. */
const PORT = Number(process.env.SNAPPY_UI_PORT ?? 5179);

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  reporter: "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    viewport: { width: 1080, height: 760 },
    trace: "retain-on-failure",
  },
  // The settings window is WebKit on macOS and Linux and Chromium-based WebView2 on Windows.
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1080, height: 760 } } },
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1080, height: 760 } } },
  ],
  webServer: {
    command: `vite --port ${PORT} --strictPort`,
    port: PORT,
    reuseExistingServer: true,
  },
});
