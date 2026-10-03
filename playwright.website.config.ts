import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./website-tests",
  fullyParallel: true,
  workers: 3,
  reporter: "list",
  outputDir: "artifacts/website-tests",
  use: {
    baseURL: "http://127.0.0.1:1431/babelhack/",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        channel: process.env.CI ? undefined : "chrome",
      },
    },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
  webServer: {
    command: "node scripts/serve-website.mjs",
    url: "http://127.0.0.1:1431/babelhack/",
    reuseExistingServer: !process.env.CI,
  },
});
