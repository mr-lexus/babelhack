import { chromium, expect } from "@playwright/test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
if (process.env.NATIVE_TEST_PROFILE !== "1" || !process.env.NATIVE_TEST_PID)
  throw new Error("Requires isolated native profile and process ID");
const run = promisify(execFile);
const menu = async (select) => {
  const args = [
    "-NoProfile",
    "-File",
    "scripts/native-tray-menu.ps1",
    "-TestProcessId",
    process.env.NATIVE_TEST_PID,
  ];
  if (select) args.push("-SelectItem", select);
  const r = await run(process.env.NATIVE_PWSH || "pwsh", args, {
    env: process.env,
    timeout: 10000,
  });
  return JSON.parse(r.stdout.trim());
};
const browser = await chromium.connectOverCDP(
  process.env.NATIVE_CDP_URL || "http://127.0.0.1:9224",
);
let main;
for (const page of browser.contexts().flatMap((c) => c.pages()))
  if (
    (await page.evaluate(
      () => window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label,
    )) === "main"
  )
    main = page;
const invoke = (name, args = {}) =>
  main.evaluate(
    ({ name, args }) => window.__TAURI_INTERNALS__.invoke(name, args),
    { name, args },
  );
const visible = (label = "main") =>
  invoke("plugin:window|is_visible", { label });
const session = () => invoke("get_session_state");
await menu("Показать окно");
let config = await invoke("get_config");
expect(config.tray_available).toBe(true);
await invoke("set_config", {
  args: {
    config: { ...config, close_to_tray: true, minimize_to_tray: false },
    deepgramKey: null,
    openaiKey: null,
  },
});
await invoke("start_session", { demo: true });
await expect.poll(async () => (await session()).active).toBe(true);
let items = await menu();
expect(items.find((i) => i.text === "Приостановить перевод")?.enabled).toBe(
  true,
);
await main
  .getByRole("button", { name: "Закрыть окно в трей", exact: true })
  .click();
await expect.poll(() => visible()).toBe(false);
expect((await session()).active).toBe(true);
const version = (await session()).version;
await expect
  .poll(async () => (await session()).version)
  .toBeGreaterThan(version);
await menu("Показать окно");
await expect.poll(() => visible()).toBe(true);
await expect(
  main.getByRole("button", { name: "Закрыть окно в трей", exact: true }),
).toBeEnabled();
// Native close requests (Alt+F4 equivalent) use the same behavior.
await invoke("plugin:window|close", { label: "main" });
await expect.poll(() => visible()).toBe(false);
await menu("Показать окно");
await expect.poll(() => visible()).toBe(true);
await main
  .getByRole("button", { name: "Свернуть в трей", exact: true })
  .click();
await expect.poll(() => visible()).toBe(false);
await menu("Приостановить перевод");
await expect.poll(async () => (await session()).paused).toBe(true);
items = await menu();
expect(items.find((i) => i.text === "Продолжить перевод")?.enabled).toBe(true);
await menu("Продолжить перевод");
await expect.poll(async () => (await session()).paused).toBe(false);
const oldOverlay = await visible("overlay");
await menu("Показать / скрыть субтитры");
await expect.poll(() => visible("overlay")).toBe(!oldOverlay);
await menu("Настройки");
await expect.poll(() => visible()).toBe(true);
await expect(
  main.getByRole("heading", { name: "Настройки", exact: true }),
).toBeVisible();
await main
  .getByRole("checkbox", { name: /Сворачивать в трей вместо панели задач/ })
  .check();
await main
  .getByRole("button", { name: "Сохранить настройки", exact: true })
  .click();
await main.reload();
await expect
  .poll(async () => (await invoke("get_config")).minimize_to_tray)
  .toBe(true);
await main.getByRole("button", { name: "Свернуть окно", exact: true }).click();
await expect.poll(() => visible()).toBe(false);
await menu("Показать окно");
await expect.poll(() => visible()).toBe(true);
await invoke("plugin:window|minimize", { label: "main" });
await expect.poll(() => visible()).toBe(false);
await menu("Завершить сессию");
await expect
  .poll(async () => (await session()).active, { timeout: 15000 })
  .toBe(false);
items = await menu();
expect(items.find((i) => i.text === "Начать перевод")?.enabled).toBe(true);
expect(items.find((i) => i.text === "Завершить сессию")?.enabled).toBe(false);
await menu("Настройки");
await main
  .getByRole("checkbox", { name: /Сворачивать в трей вместо панели задач/ })
  .uncheck();
await main
  .getByRole("button", { name: "Сохранить настройки", exact: true })
  .click();
await main.getByRole("button", { name: "Свернуть окно", exact: true }).click();
await expect
  .poll(() => invoke("plugin:window|is_minimized", { label: "main" }))
  .toBe(true);
expect(await visible()).toBe(true);
await menu("Показать окно");
await expect
  .poll(() => invoke("plugin:window|is_minimized", { label: "main" }))
  .toBe(false);
await main.screenshot({
  path: "artifacts/native-tray-settings.png",
  fullPage: true,
});
// Explicit exit must ignore close-to-tray and drain even a hidden active session.
await invoke("start_session", { demo: true });
await invoke("tray_action", { action: "hide" });
const closed = main.waitForEvent("close");
await menu("Выйти из приложения");
await closed;
await browser.close();
console.log(
  "PASS: actual Windows tray menu state/actions; close and native close hide without stopping; titlebar stays usable; pause/resume/overlay/settings; minimize policy and persistence; stop; exit while hidden and active.",
);
