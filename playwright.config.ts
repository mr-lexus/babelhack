import { defineConfig, devices } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  workers: 3,
  retries: 0,
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "artifacts/test-report" }],
  ],
  use: {
    baseURL: "http://127.0.0.1:1420",
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
    {
      name: "webkit",
      fullyParallel: false,
      use: { ...devices["Desktop Safari"] },
    },
  ],
  webServer: {
    command: "npm run dev -- --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
  },
});
