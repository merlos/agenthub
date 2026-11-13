import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./screenshots",
  testMatch: "demo-screenshots.ts",
  outputDir: "artifacts/playwright",
  fullyParallel: false,
  workers: 1,
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:1420",
    browserName: "chromium",
    viewport: { width: 1220, height: 790 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    locale: "en-US",
    timezoneId: "UTC",
    reducedMotion: "reduce",
  },
  webServer: {
    command: "npm run demo -- --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
  },
});
