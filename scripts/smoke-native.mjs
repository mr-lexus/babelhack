import { chromium, expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
const browser = await chromium.connectOverCDP(
  process.env.NATIVE_CDP_URL || "http://127.0.0.1:9224",
);
if (!process.env.NATIVE_TEST_PROFILE)
  throw new Error(
    "Set NATIVE_TEST_PROFILE=1 only for an isolated app instance",
  );
const pages = browser.contexts().flatMap((c) => c.pages());
let main, overlay;
for (const p of pages) {
  await p.waitForFunction(() => document.readyState !== "loading");
  await p.waitForFunction(
    () => !!window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
  );
  const label = await p.evaluate(
    () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
  );
  if (label === "main") main = p;
  if (label === "overlay") overlay = p;
}
if (!main || !overlay) throw new Error("Both native windows must exist");
const errors = [];
main.on("pageerror", (e) => errors.push(e.message));
overlay.on("pageerror", (e) => errors.push(e.message));
await main
  .getByRole("button", { name: "Живой перевод", exact: true })
  .first()
  .click();
await expect(
  main.getByRole("heading", { name: "Живой перевод", exact: true }),
).toBeVisible();
const status = await main.evaluate(async () => {
  const c = await window.__TAURI_INTERNALS__.invoke("get_config");
  const devices = await window.__TAURI_INTERNALS__.invoke(
    "list_output_devices",
  );
  return {
    os: c.platform?.os,
    stable_device_ids: devices.every(
      (d) => typeof d.id === "string" && typeof d.name === "string",
    ),
    has_deepgram: c.has_deepgram,
    has_openai: c.has_openai,
    model: c.openai_model,
    device_count: devices.length,
  };
});
console.log("Native preflight", JSON.stringify(status));
expect(status.os).toBe(
  process.platform === "win32"
    ? "windows"
    : process.platform === "darwin"
      ? "macos"
      : "linux",
);
expect(status.stable_device_ids).toBe(true);
const windowState = (command) =>
  main.evaluate(
    (command) =>
      window.__TAURI_INTERNALS__.invoke("plugin:window|" + command, {
        label: "main",
      }),
    command,
  );
expect(await windowState("is_decorated")).toBe(false);
await expect(main.locator(".window-titlebar .brand-icon")).toHaveAttribute(
  "src",
  "/brand/app-icon.svg",
);
await main
  .getByRole("button", { name: "Развернуть окно", exact: true })
  .click();
await expect.poll(() => windowState("is_maximized")).toBe(true);
await main
  .getByRole("button", { name: "Восстановить размер окна", exact: true })
  .click();
await expect.poll(() => windowState("is_maximized")).toBe(false);
await main
  .locator(".window-drag-area")
  .dispatchEvent("mousedown", { button: 0, detail: 2 });
await expect.poll(() => windowState("is_maximized")).toBe(true);
await main
  .getByRole("button", { name: "Восстановить размер окна", exact: true })
  .click();
await expect.poll(() => windowState("is_maximized")).toBe(false);
await main.getByRole("button", { name: "Свернуть окно", exact: true }).click();
await expect.poll(() => windowState("is_minimized")).toBe(true);
await windowState("unminimize");
await expect.poll(() => windowState("is_minimized")).toBe(false);
await expect(main.locator(".window-resize-edge")).toHaveCount(8);
await main.screenshot({
  path: "artifacts/native-custom-frame.png",
  fullPage: true,
});
console.log(
  "Native frameless window, custom minimize/maximize/restore and double-click: PASS",
);
await main.getByRole("button", { name: "Демо", exact: true }).click();
await expect(
  overlay.locator('.caption-live[data-provisional="true"] p'),
).not.toBeEmpty();
await overlay.screenshot({ path: "artifacts/native-streaming-draft.png" });
await expect(main.locator(".transcript-entry").first()).toContainText(
  "Расскажите немного о себе.",
  { timeout: 15000 },
);
await main.getByRole("button", { name: "Пауза", exact: true }).click();
await expect(main.locator(".status-badge")).toHaveText("На паузе");
await expect(overlay.locator(".overlay-content")).toContainText("Спасибо", {
  timeout: 5000,
});
await overlay.reload();
await expect(overlay.locator(".overlay-content")).toContainText("Спасибо", {
  timeout: 5000,
});
await main.screenshot({ path: "artifacts/native-live.png", fullPage: true });
await overlay.screenshot({ path: "artifacts/native-overlay.png" });
await main.reload();
await expect(main.locator(".status-badge")).toHaveText("На паузе");
await main.getByRole("button", { name: "Продолжить", exact: true }).click();
await main.getByRole("button", { name: "Завершить", exact: true }).click();
await expect(main.locator(".status-badge")).toHaveText("Готов к работе", {
  timeout: 15000,
});
await expect(main.locator(".transcript-entry").first()).toContainText(
  "Спасибо",
);
await main
  .getByRole("button", { name: "Настройки", exact: true })
  .first()
  .click();
await expect(
  main.getByRole("heading", { name: "Подключения", exact: true }),
).toBeVisible();
await main.screenshot({
  path: "artifacts/native-settings.png",
  fullPage: true,
});
await main.getByLabel("Переводить на", { exact: true }).selectOption("de");
await main.getByRole("button", { name: "Сохранить настройки" }).click();
await expect(main.getByRole("status")).toContainText("Настройки сохранены");
await main.reload();
await main
  .getByRole("button", { name: "Настройки", exact: true })
  .first()
  .click();
await expect(main.getByLabel("Переводить на", { exact: true })).toHaveValue(
  "de",
);
// Only synthetic history is written, in the explicitly selected isolated profile.
const profile = process.env.INTERVIEW_TRANSLATOR_DATA_DIR;
if (!profile || !path.isAbsolute(profile))
  throw new Error(
    "An absolute INTERVIEW_TRANSLATOR_DATA_DIR matching the test app is required",
  );
const fixtureId = "123456789-98765";
const fixture = {
  id: fixtureId,
  title: "Native smoke fixture",
  started_at: 123456789,
  ended_at: 123460000,
  source_language: "en",
  target_language: "ru",
  entries: [
    {
      id: 1,
      timestamp_ms: 100,
      source: "Hello",
      translation: "Привет",
      error: null,
    },
    {
      id: 2,
      timestamp_ms: 200,
      source: "Unfinished phrase",
      translation: "",
      error: null,
    },
  ],
};
await mkdir(path.join(profile, "sessions"), { recursive: true });
await writeFile(
  path.join(profile, "sessions", fixtureId + ".json"),
  JSON.stringify(fixture),
);
await main
  .getByRole("button", { name: "История сессий", exact: true })
  .first()
  .click();
await main.getByLabel("Найти сессию", { exact: true }).fill("Native smoke");
await main
  .locator(".session-row > button:first-child")
  .filter({ hasText: fixture.title })
  .click();
await expect(main.locator(".transcript-entry").first()).toContainText("Привет");
await expect(main.locator(".transcript-entry").nth(1)).toContainText(
  "Перевод был прерван",
);
await main
  .getByRole("button", { name: "Удалить сессию " + fixture.title, exact: true })
  .click();
await main
  .getByRole("dialog")
  .getByRole("button", { name: "Удалить", exact: true })
  .click();
await expect(
  main.locator(".session-row").filter({ hasText: fixture.title }),
).toHaveCount(0);
await expect
  .poll(async () =>
    main.evaluate(async (id) => {
      const list = await window.__TAURI_INTERNALS__.invoke("list_sessions");
      return list.some((r) => r.id === id);
    }, fixtureId),
  )
  .toBe(false);
console.log(
  "Native demo/pause/resume/stop, both reloads, settings persistence, history/search/delete and device enumeration: PASS",
);
console.log("Page errors", JSON.stringify(errors));
expect(errors).toEqual([]);
if (process.env.NATIVE_CLOSE_ON_SUCCESS) {
  const closed = main.waitForEvent("close");
  await main
    .evaluate(() =>
      window.__TAURI_INTERNALS__.invoke("tray_action", { action: "quit" }),
    )
    .catch(() => {});
  await closed;
  console.log("Native graceful main-window shutdown: PASS");
}
await browser.close();
