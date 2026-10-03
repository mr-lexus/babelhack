import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

// Render the code-native card using the same assets as the landing page.
// No network calls, live user content, or UI recreation are involved.
let html = await readFile(
  new URL("./social-card.html", import.meta.url),
  "utf8",
);
const mime = { png: "image/png", svg: "image/svg+xml", woff2: "font/woff2" };
for (const [, name] of html.matchAll(/\{\{([a-z0-9.-]+)\}\}/g)) {
  const bytes = await readFile(
    new URL(`../website/assets/${name}`, import.meta.url),
  );
  const type = mime[name.split(".").pop()];
  if (!type) throw new Error(`Unsupported asset: ${name}`);
  html = html.replaceAll(
    `{{${name}}}`,
    `data:${type};base64,${bytes.toString("base64")}`,
  );
}
const browser = await chromium.launch({
  channel: process.env.CI ? undefined : "chrome",
});
try {
  const page = await browser.newPage({
    viewport: { width: 1200, height: 630 },
    deviceScaleFactor: 1,
  });
  await page.route("**/*", (route) => route.abort());
  await page.setContent(html);
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map((image) => image.decode()));
    if (!document.fonts.check("600 65px Onest", "Субтитры"))
      throw new Error("Card font failed to load");
  });
  await page.screenshot({
    path: fileURLToPath(
      new URL("../website/assets/social-card-v1.png", import.meta.url),
    ),
    type: "png",
  });
  console.log("Rendered website/assets/social-card-v1.png (1200 × 630)");
} finally {
  await browser.close();
}
