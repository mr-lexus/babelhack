# Лендинг Babel Hack

Публичная страница: <https://mr-lexus.github.io/babelhack/>.

Исходники находятся в `website/`. Это статический HTML/CSS/JavaScript без сборки, сторонних виджетов, аналитики, форм сбора данных и внешних шрифтов. Сайт не запрашивает API-ключи. Шрифты и изображения загружаются с того же домена; GitHub Pages обслуживает страницу, GitHub Releases — загрузки приложения.

## Локальный просмотр и проверки

```sh
npm ci
node scripts/serve-website.mjs
# http://127.0.0.1:1431/babelhack/

npx playwright install chromium webkit
npx playwright test --config=playwright.website.config.ts
npx prettier --check website/index.html website/styles.css website/site.js website-tests scripts/serve-website.mjs playwright.website.config.ts
```

Локальный сервер доступен только на loopback и раздаёт только `website/`, с тем же префиксом `/babelhack/`, что и GitHub Pages. Playwright сам запускает его, если сервер ещё не работает.

Проверки покрывают 12 вариантов скачивания, переключение платформ с клавиатуры, галерею, модальное окно и восстановление фокуса, загрузку изображений, отсутствие горизонтального скролла на ширинах 320/390/768/1440, автоматическую проверку WCAG A/AA через axe, FAQ и базовые загрузки без JavaScript. Автоматические проверки доступности не заменяют ручную проверку.

## Анимации

Нативные CSS-анимации и Web Animations API, без дополнительных библиотек: появление секций, переходы галереи и выбора ОС, открытие скриншотов и ответов FAQ. На первом экране медленно движутся подсветка и фотография; в секции загрузки — контурные линии фона. Сам скриншот оверлея и его текст неподвижны.

Анимации запускаются автоматически, без переключателей и сохранённых настроек. При системном `prefers-reduced-motion` остаются короткие появления через прозрачность: фотография неподвижна, фон плавно появляется один раз без вращения. В обычном режиме фоновые анимации приостанавливаются за пределами экрана и в скрытой вкладке. Клавиатурный фокус прекращает появление элемента, чтобы не попасть на невидимую кнопку. Без JavaScript контент и загрузки остаются доступны.

Playwright дополнительно проверяет автоматический запуск движения без кнопок, паузу за пределами экрана, облегчённые анимации при системном уменьшении движения и переключение галереи.

## Публикация

`.github/workflows/website.yml` проверяет страницу в Chromium и WebKit, затем публикует только каталог `website/` в GitHub Pages. Настройка репозитория: **Settings → Pages → Source → GitHub Actions**. Pull request проходит проверки без публикации. Секретов для сайта не требуется; используются штатные разрешения `pages: write` и `id-token: write` только в задании публикации.

При изменении CSS/JavaScript обновлять параметр `?v=` у обоих файлов в `index.html`. Он связывает HTML с нужной версией ресурсов: браузер с открытой прежней страницей не должен смешивать новую разметку с закешированными стилями или скриптом.

Изменения сайта сами по себе не пересобирают desktop-приложение. Первый коммит, меняющий также desktop workflow, закономерно проходит его проверки.

## Обновление релиза

Ссылки намеренно закреплены на опубликованном **v0.7.0**. Они не зависят от GitHub API при открытии страницы. После публикации новой версии:

1. Обновить версию и ссылки в `website/index.html`, `website/site.js` и ожидаемые URL в `website-tests/landing.spec.ts`.
2. Сверить имена и наличие всех 12 пакетов и `SHA256SUMS.txt` в GitHub Releases.
3. Перепроверить требования платформ, статус подписи, ограничения и текст FAQ по реальному релизу.
4. При изменении интерфейса переснять скриншоты. Запустить проверки и просмотреть desktop/mobile варианты.
5. Отправить изменения в `main`; публикация выполняется только после успешных тестов.

## Изображения и шрифт

- `assets/app-icon.svg` — существующий знак приложения из `public/brand/app-icon.svg`.
- `assets/app-overlay.png`, `app-session.png`, `app-settings.png` — настоящие скриншоты релизной Windows-сборки Babel Hack 0.7.0, снятые через WebView2 в отдельном тестовом профиле. Демо EN → RU, без записи реального звука и без API-запросов. Содержимое интерфейса не перерисовано. У оверлея сохранён прозрачный фон; фотография расположена под ним средствами CSS. Шрифт и размеры оверлея выбраны через поддерживаемые настройки приложения.
- `assets/fjord.png` — декоративная иллюстрация, созданная встроенным imagegen для этой страницы. Это фон, а не изображение собеседника или реальной трансляции. На странице кадр подписан как демо с иллюстративным фоном.
- `assets/onest-*.woff2` — Onest, локальные Cyrillic/Latin подмножества Google Fonts. Лицензия SIL Open Font License 1.1 сохранена в `assets/Onest-OFL.txt`. Источник: <https://github.com/google/fonts/tree/main/ofl/onest>.

Промпт фона (встроенный imagegen, без CLI):

> Use case: photorealistic-natural. Asset type: a premium desktop wallpaper used behind a real subtitle application screenshot on the Babel Hack product landing page. Generate a wide cinematic landscape photograph, aspect ratio 16:9, at least 1536px wide. A dramatic almost-black volcanic mountain ridge rising from a still Icelandic fjord at blue hour, distant layered mountains, delicate bands of lavender and pale peach in the sky. Minimal editorial landscape photography, sophisticated restrained color, fine natural texture, believable photographic realism, luminous but quiet. Main mountain on right half; left half open water and subtle sky with generous calm negative space; lower third dark and uncluttered so a separately composited translucent app overlay is readable. Palette charcoal, slate blue, muted lavender and warm pale light. No people, no houses, no screens, no devices, no interface, no letters, no logos, no watermarks. This is only a background photograph; do not render the app or any text.

## Редакционные правила

Не добавлять выдуманные отзывы, пользовательские показатели, гарантии точности/задержки, обещания офлайн-перевода, микрофона или двустороннего перевода. Бесплатна программа, а не расходы Deepgram/OpenAI. Различать демо и живой перевод, наличие сборки и проверку на физическом устройстве. Не скрывать требования API, передачу данных сервисам, ограничения macOS/Wayland и статус подписей установщиков.
