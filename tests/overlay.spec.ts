import { expect, test, type Page } from "@playwright/test";

async function patch(page: Page, patch: Record<string, unknown>) {
  await page.evaluate(async (patch) => {
    const url = performance
      .getEntriesByType("resource")
      .map((r) => r.name)
      .find((n) => /\/src\/session-store\.ts(?:\?|$)/.test(n));
    if (!url) throw new Error("Session store was not loaded");
    const { sessionStore: store } = await import(url);
    store.accept({
      ...store.state,
      version: store.state.version + 1,
      ...patch,
    });
  }, patch);
}

test("overlay isolates React commits and retains completed caption DOM nodes", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (window as any).renderCounts = {};
    window.addEventListener("overlay-render", (e: any) => {
      const counts = (window as any).renderCounts;
      counts[e.detail.id] = (counts[e.detail.id] || 0) + 1;
    });
  });
  await page.goto("/?overlay=1&profile=1");
  await expect(page.locator(".overlay-shell")).toBeVisible();
  await patch(page, {
    session_id: "render-test",
    started_at: Date.now(),
    active: true,
    cues: [
      { id: 1, text: "Первый готовый фрагмент." },
      { id: 2, text: "Второй готовый фрагмент." },
    ],
    current: "Новая",
    translation_source: "A new clause",
    provisional: true,
  });
  await expect(page.locator(".caption-live p")).toHaveText("Новая");
  await page.evaluate(() => {
    (window as any).renderCounts = {};
    (window as any).savedCaption = document.querySelector('[data-cue-id="2"]');
  });
  for (let i = 0; i < 10; i++)
    await patch(page, { audio_level: i / 10, pending: i % 3 });
  expect(await page.evaluate(() => (window as any).renderCounts)).toEqual({});
  await patch(page, { current: "Новая фраза", source: "Newer ASR hypothesis" });
  await expect(page.locator(".caption-live p")).toHaveText("Новая фраза");
  const counts = await page.evaluate(() => (window as any).renderCounts);
  expect(counts.live).toBeGreaterThan(0);
  expect(counts.history ?? 0).toBe(0);
  expect(counts.status ?? 0).toBe(0);
  expect(counts.source ?? 0).toBe(0); // original remains aligned to the translation
  expect(counts["cue-2"] ?? 0).toBe(0);
  await patch(page, {
    cues: [
      { id: 1, text: "Первый готовый фрагмент." },
      { id: 2, text: "Второй готовый фрагмент." },
      { id: 3, text: "Новая фраза" },
    ],
    current: "",
    provisional: false,
  });
  await expect(page.locator(".caption-final")).toHaveCount(3);
  expect(
    await page.evaluate(
      () =>
        document.querySelector('[data-cue-id="2"]') ===
        (window as any).savedCaption,
    ),
  ).toBe(true);
  expect(
    await page.evaluate(() => (window as any).renderCounts["cue-2"] ?? 0),
  ).toBe(0);
  await patch(page, {
    cues: [
      { id: 2, text: "Второй готовый фрагмент." },
      { id: 3, text: "Новая фраза" },
      { id: 4, text: "Следующая фраза" },
    ],
  });
  expect(
    await page.evaluate(
      () =>
        document.querySelector('[data-cue-id="2"]') ===
        (window as any).savedCaption,
    ),
  ).toBe(true);
  await page.screenshot({ path: "artifacts/overlay-streaming.png" });
});

test("out of order snapshots cannot restore obsolete text or a previous session", async ({
  page,
}) => {
  await page.goto("/?overlay=1");
  await patch(page, {
    session_id: "new",
    started_at: 200,
    version: 10,
    current: "Актуальный перевод",
  });
  await patch(page, {
    session_id: "new",
    started_at: 200,
    version: 9,
    current: "Устаревший перевод",
  });
  await expect(page.locator(".caption-live p")).toHaveText(
    "Актуальный перевод",
  );
  await patch(page, {
    session_id: "old",
    started_at: 100,
    version: 999,
    current: "Предыдущая сессия",
  });
  await expect(page.locator(".caption-live p")).toHaveText(
    "Актуальный перевод",
  );
});

for (const width of [420, 1100]) {
  test(`coherent captions wrap as paragraphs and preserve failed source at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 650 });
    await page.goto("/?overlay=1");
    const sentence =
      "Мы можем не только усилить волны глубокого сна, но и почти вдвое увеличить пользу сна для памяти.";
    await patch(page, {
      session_id: "coherent-test",
      started_at: Date.now(),
      active: true,
      cues: [{ id: 1, text: sentence }],
      current: "Теперь нужно понять, можно ли использовать эту технологию дома",
      provisional: true,
    });
    await expect(page.locator(".caption-final")).toHaveCount(1);
    await expect(page.locator(".caption-final")).toHaveText(sentence);
    expect(
      await page
        .locator(".caption-final")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    ).toBe(true);
    await patch(page, {
      cues: [
        { id: 1, text: sentence },
        {
          id: 2,
          text: "The complete original sentence is available.",
          untranslated: true,
        },
      ],
      current: "",
      message: "Перевод: временный сбой соединения.",
    });
    await expect(
      page.locator('[data-cue-id="2"] .caption-fallback-label'),
    ).toHaveText("Оригинал · перевод недоступен");
    await expect(page.locator('[data-cue-id="2"]')).toContainText(
      "The complete original sentence is available.",
    );
    // A metadata-only update must not be swallowed by structural sharing.
    await patch(page, {
      cues: [
        { id: 1, text: sentence },
        {
          id: 2,
          text: "The complete original sentence is available.",
          untranslated: false,
        },
      ],
      message: null,
    });
    await expect(page.locator(".caption-fallback-label")).toHaveCount(0);
    await expect(page.locator(".overlay-error")).toHaveCount(0);
    await page.screenshot({ path: `artifacts/coherent-overlay-${width}.png` });
  });
}

test("reading earlier captions is not interrupted by streaming updates", async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 300 });
  await page.goto("/?overlay=1");
  await patch(page, {
    session_id: "scroll-test",
    started_at: Date.now(),
    active: true,
    cues: [1, 2, 3].map((id) => ({
      id,
      text:
        `Предложение ${id}. ` +
        "Это длинное цельное предложение для проверки прокрутки и чтения истории. ".repeat(
          3,
        ),
    })),
    current: "Начало следующей фразы",
    provisional: true,
  });
  const body = page.locator(".overlay-content");
  await expect
    .poll(() => body.evaluate((el) => el.scrollTop))
    .toBeGreaterThan(100);
  await body.evaluate((el) => {
    el.scrollTop = 0;
    el.dispatchEvent(new Event("scroll"));
  });
  await patch(page, {
    current: "Начало следующей фразы постепенно дополняется новыми словами",
  });
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  expect(await body.evaluate((el) => el.scrollTop)).toBe(0);
});
