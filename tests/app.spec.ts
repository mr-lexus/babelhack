import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test("macOS shows Keychain guidance and disables unsupported overlay transparency", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(async () => {
    const url = performance
      .getEntriesByType("resource")
      .map((r) => r.name)
      .find((name) => /\/src\/api\.ts(?:\?|$)/.test(name));
    if (!url) throw new Error("App API module was not loaded");
    const { command } = await import(url);
    const config = await command("get_config");
    await command("set_config", {
      args: {
        config: {
          ...config,
          platform: {
            os: "macos",
            label: "macOS",
            credential_store: "macOS Keychain",
            audio_backend: "CoreAudio Process Tap",
            audio_help: "Разрешите запись системного аудио. macOS 14.6+.",
            overlay_shortcut: "⌘ ⌥ T",
            wayland: false,
            overlay_transparency: false,
          },
        },
      },
    });
  });
  await expect(page.locator(".sidebar-version")).toContainText("MACOS");
  await page
    .getByRole("button", { name: "Настройки", exact: true })
    .first()
    .click();
  await expect(page.getByText(/Ключи хранятся в macOS Keychain/)).toBeVisible();
  await expect(page.getByLabel(/Непрозрачность фона/)).toBeDisabled();
  await expect(page.getByLabel(/Непрозрачность фона/)).toHaveAttribute(
    "max",
    "1",
  );
  await expect(page.getByText(/На macOS фон непрозрачный/)).toBeVisible();
});

test("Wayland presents desktop limitations and Secret Service failures clearly", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(async () => {
    const url = performance
      .getEntriesByType("resource")
      .map((r) => r.name)
      .find((name) => /\/src\/api\.ts(?:\?|$)/.test(name));
    if (!url) throw new Error("App API module was not loaded");
    const { command } = await import(url);
    const config = await command("get_config");
    await command("set_config", {
      args: {
        config: {
          ...config,
          credential_error:
            "Хранилище ключей недоступно: разблокируйте Secret Service.",
          platform: {
            os: "linux",
            label: "Linux",
            credential_store: "Secret Service",
            audio_backend: "PulseAudio / PipeWire monitor",
            audio_help: "Нужен pipewire-pulse.",
            overlay_shortcut: "Ctrl Alt T",
            wayland: true,
            overlay_transparency: true,
          },
        },
      },
    });
  });
  await expect(page.locator(".overlay-button kbd")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Настройки", exact: true })
    .first()
    .click();
  await expect(page.getByRole("alert")).toContainText("Secret Service");
  await expect(page.getByText(/В Wayland положение/)).toBeVisible();
});

test("onboarding blocks live capture without credentials and offers demo", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Живой перевод", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Начать перевод", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Демо", exact: true }),
  ).toBeEnabled();
  expect(errors).toEqual([]);
});

test("demo supports pause, resume, search, export, stop and a clean restart", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Демо", exact: true }).click();
  await expect(page.locator(".transcript-entry")).toHaveCount(1, {
    timeout: 12000,
  });
  await page.getByRole("button", { name: "Пауза", exact: true }).click();
  await expect(page.locator(".status-badge")).toHaveText("На паузе");
  const source = await page.locator(".source-card > p").innerText();
  await page.waitForTimeout(450);
  await expect(page.locator(".source-card > p")).toHaveText(source);
  await page.getByRole("button", { name: "Продолжить", exact: true }).click();
  await page.getByLabel("Поиск по разговору").fill("несуществующая фраза");
  await expect(
    page.getByText("Ничего не найдено. Попробуйте другое слово."),
  ).toBeVisible();
  await page.getByLabel("Поиск по разговору").fill("Thanks");
  await expect(page.locator(".transcript-entry")).toHaveCount(1);
  await page.locator(".export-menu summary").click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "JSON", exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/babelhack-\d+\.json/);
  const stream = await file.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream!) chunks.push(chunk);
  const exported = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  expect(exported.entries[0].translation).toContain("Спасибо");
  await page.getByRole("button", { name: "Завершить", exact: true }).click();
  await expect(page.locator(".status-badge")).toHaveText("Готов к работе");
  await expect(page.locator(".transcript-entry").first()).toContainText(
    "Спасибо",
  );
  await page.getByRole("button", { name: "Демо", exact: true }).click();
  await expect(page.locator(".transcript-entry")).toHaveCount(0);
  await page.getByRole("button", { name: "Завершить", exact: true }).click();
});

test("settings draft survives navigation and saved languages update the workspace", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: "Настройки", exact: true })
    .first()
    .click();
  await page
    .getByRole("combobox", { name: "Переводить на", exact: true })
    .selectOption("de");
  await page
    .getByLabel("Словарь перевода")
    .fill("API = API\ndeployment = развёртывание");
  await page
    .getByRole("button", { name: "Живой перевод", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Настройки", exact: true })
    .first()
    .click();
  await expect(
    page.getByRole("combobox", { name: "Переводить на", exact: true }),
  ).toHaveValue("de");
  await page
    .getByRole("button", { name: "Сохранить настройки", exact: true })
    .click();
  await expect(
    page.getByRole("status").filter({ hasText: "Настройки сохранены" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Живой перевод", exact: true })
    .click();
  await expect(page.locator(".language-pair")).toContainText("Deutsch");
});

for (const width of [390, 760, 1180]) {
  test(
    "layout and navigation remain usable at " + width + "px",
    async ({ page }) => {
      await page.setViewportSize({ width, height: 850 });
      await page.goto("/");
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      const nav =
        width < 650 ? page.locator(".mobile-nav") : page.locator(".sidebar");
      await nav.getByRole("button", { name: "Настройки", exact: true }).click();
      await expect(
        page.getByRole("heading", { name: "Подключения", exact: true }),
      ).toBeVisible();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    },
  );
}

test("main workspace and settings satisfy automated WCAG AA checks", async ({
  page,
}) => {
  await page.goto("/");
  let result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
    .analyze();
  expect(
    result.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => ({
        target: n.target,
        summary: n.failureSummary,
      })),
    })),
  ).toEqual([]);
  await page
    .getByRole("button", { name: "Настройки", exact: true })
    .first()
    .click();
  result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
    .analyze();
  expect(
    result.violations.map((v) => ({
      id: v.id,
      nodes: v.nodes.map((n) => ({
        target: n.target,
        summary: n.failureSummary,
      })),
    })),
  ).toEqual([]);
});
