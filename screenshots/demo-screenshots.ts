import { expect, test } from "@playwright/test";

const captures = [
  { name: "overview", path: "/", heading: "Overview", readyText: "8,946" },
  { name: "applications", path: "/applications", heading: "Applications", readyText: "OpenCode" },
  { name: "providers", path: "/providers", heading: "Providers", readyText: "OpenAI" },
  { name: "openai-statistics", path: "/providers/demo-openai/stats", heading: "OpenAI statistics", readyText: "Current billing period" },
  { name: "request-logs", path: "/logs", heading: "Request logs", readyText: "gpt-5-mini" },
  { name: "settings", path: "/settings", heading: "Settings", readyText: "~/.config/agenthub-demo/config.toml" },
] as const;

for (const capture of captures) {
  test(`capture ${capture.name}`, async ({ page }) => {
    await page.goto(capture.path);
    await expect(page.getByRole("heading", { level: 1, name: capture.heading })).toBeVisible();
    await expect(page.getByText(capture.readyText, { exact: false }).first()).toBeVisible();
    await page.screenshot({ path: `docs/assets/${capture.name}.png` });
  });
}
