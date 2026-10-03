import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test("real screenshots load, gallery and modal work with keyboard", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("./");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Слышать мир.Понимать больше.",
  );
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await expect(page.locator("#workspace-image")).toHaveAttribute(
    "src",
    "assets/app-settings.png",
  );
  const trigger = page.getByRole("button", {
    name: "Увеличить скриншот приложения",
    exact: true,
  });
  await trigger.click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.locator("#screenshot-full")).toHaveAttribute(
    "src",
    "assets/app-settings.png",
  );
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).not.toBeVisible();
  await expect(trigger).toBeFocused();
  await page
    .getByRole("button", { name: "Живой перевод", exact: true })
    .click();
  await expect(page.locator("#workspace-image")).toHaveAttribute(
    "src",
    "assets/app-session.png",
  );
  await page.locator("#workspace-image").scrollIntoViewIfNeeded();
  await expect
    .poll(() =>
      page
        .locator("img[src]")
        .evaluateAll((images) =>
          images.every(
            (image) =>
              (image as HTMLImageElement).complete &&
              (image as HTMLImageElement).naturalWidth > 0,
          ),
        ),
    )
    .toBe(true);
  expect(errors).toEqual([]);
});

test("download selectors map all 12 release packages and support keyboard tabs", async ({
  page,
}) => {
  await page.goto("./");
  const base =
    "https://github.com/mr-lexus/babelhack/releases/download/v0.7.0/BabelHack-0.7.0-";
  await page.getByRole("tab", { name: "Windows", exact: true }).click();
  await expect(
    page.locator('[data-asset="windows-x64-setup.exe"]'),
  ).toHaveAttribute("href", base + "windows-x64-setup.exe");
  await expect(page.locator('[data-asset="windows-x64.exe"]')).toHaveAttribute(
    "href",
    base + "windows-x64.exe",
  );
  await page
    .getByRole("tab", { name: "Windows", exact: true })
    .press("ArrowRight");
  await expect(
    page.getByRole("tab", { name: "macOS", exact: true }),
  ).toBeFocused();
  await expect(page.locator("#panel-macos")).toBeVisible();
  for (const arch of ["arm64", "x64"]) {
    await page.locator("#mac-arch").selectOption(arch);
    await expect(page.locator("#mac-download")).toHaveAttribute(
      "href",
      base + `macos-${arch}.dmg`,
    );
    await expect(page.locator("#mac-zip")).toHaveAttribute(
      "href",
      base + `macos-${arch}.app.zip`,
    );
  }
  await page.getByRole("tab", { name: "Linux", exact: true }).click();
  for (const arch of ["x64", "arm64"])
    for (const format of ["deb", "rpm", "AppImage"]) {
      await page.locator("#linux-arch").selectOption(arch);
      await page.locator("#linux-format").selectOption(format);
      await expect(page.locator("#linux-download")).toHaveAttribute(
        "href",
        base + `linux-${arch}.${format}`,
      );
    }
});

test("responsive layouts have no horizontal overflow or WCAG A/AA violations", async ({
  page,
}) => {
  await page.goto("./");
  for (const width of [320, 390, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await expect
      .poll(() =>
        page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      )
      .toBe(true);
    for (const platform of ["Windows", "macOS", "Linux"]) {
      await page.getByRole("tab", { name: platform, exact: true }).click();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    }
  }
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    const scan = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
      .analyze();
    expect(scan.violations).toEqual([]);
  }
});

test("download and FAQ remain usable without JavaScript", async ({
  browser,
}) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto("http://127.0.0.1:1431/babelhack/");
  await expect(page.locator("#panel-windows")).toBeVisible();
  await expect(page.locator("#panel-macos")).toBeVisible();
  await expect(page.locator("#panel-linux")).toBeVisible();
  await expect(
    page.getByRole("link", { name: "все сборки на GitHub" }),
  ).toBeVisible();
  await page
    .getByText("Можно пользоваться без интернета?", { exact: true })
    .click();
  await expect(
    page.getByText("Живой перевод требует подключения", { exact: false }),
  ).toBeVisible();
  await context.close();
});
