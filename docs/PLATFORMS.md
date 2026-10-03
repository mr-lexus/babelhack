# Платформы Babel Hack

## Матрица выпуска

| Цель | Runner CI | Захват звука | Пакеты |
|---|---|---|---|
| Windows 10/11 x64 | windows-2022 | WASAPI loopback | NSIS EXE, отдельный EXE |
| macOS 14.6+ Apple Silicon | macos-15 | CoreAudio Process Tap | DMG, app.zip |
| macOS 14.6+ Intel | macos-15-intel | CoreAudio Process Tap | DMG, app.zip |
| Linux x64 | ubuntu-22.04 | PulseAudio / pipewire-pulse monitor | DEB, RPM, AppImage |
| Linux ARM64 | ubuntu-22.04-arm | PulseAudio / pipewire-pulse monitor | DEB, RPM, AppImage |

[Workflow](../.github/workflows/desktop.yml) выполняется на нативных хостах. Успешный статус подтверждает компиляцию, Rust-тесты и упаковку. Linux дополнительно проверяет PCM виртуального аудиовыхода и Secret Service. Браузерные тесты Chromium/WebKit выполняются отдельно на Linux x64. Нативное Windows-демо проверяется локальным smoke-сценарием.

CI не подтверждает работу всех физических аудиоустройств, macOS TCC, многомониторных конфигураций или Wayland compositor. При сообщении об ошибке приложите ОС/архитектуру, тип сессии X11/Wayland, устройство и обезличенную диагностику.

## Windows

Запустите `BabelHack-<version>-windows-x64-setup.exe`. Установщик использует WebView2 Runtime; если его нет, нужен доступ к сети для установки. Отдельный EXE тоже требует WebView2. Он использует системный профиль, а не переносит настройки рядом с собой.

Выпуск без Authenticode. Windows может показать SmartScreen/неизвестного издателя: проверьте источник и SHA256SUMS до запуска. Для сборки нужны Node.js 22+, Rust MSVC и Visual Studio Build Tools с C++/Windows SDK.

```powershell
npm ci
npm run build:windows
Get-FileHash .\BabelHack-0.7.0-windows-x64-setup.exe -Algorithm SHA256
```

WASAPI захватывает выбранный выход, не микрофон. Трей: левая кнопка показывает окно, правая — меню. Крестик по умолчанию скрывает главное окно; полный выход находится в меню трея.

## macOS

Выберите DMG своей архитектуры и перенесите Babel Hack в Applications. Альтернатива — распаковать `.app.zip` туда же. Требуется macOS 14.6+ и разрешение на системный аудиозахват. Запускайте `.app`, чтобы TCC связывал разрешение с приложением.

Сборки имеют ad-hoc подпись, но не Developer ID и notarization. После первой попытки открытия macOS может потребовать **System Settings → Privacy & Security → Open Anyway**. Используйте этот путь только для проверенного файла из релиза. Ad-hoc подпись не подтверждает личность издателя. [Документация Tauri](https://v2.tauri.app/distribute/sign/macos/).

При отказе в аудиозахвате откройте Privacy & Security → Screen & System Audio Recording (название зависит от macOS), разрешите доступ и перезапустите. `Info.plist` объясняет передачу аудио Deepgram и текста OpenAI.

```sh
xcode-select --install
npm ci
npm run build:desktop
```

Rust stable и Node.js 22+ устанавливаются отдельно. Текущий хост определяет архитектуру; CI собирает обе. Минимальная версия задана в bundle-конфиге и MACOSX_DEPLOYMENT_TARGET.

Process tap явно создаётся для выбранного output UID, в том числе для duplex-устройств; затем создаётся приватный aggregate. Stream освобождается до aggregate/tap, включая путь ошибки. Микрофон не используется. Оверлей непрозрачный: private WKWebView API не включены. Сочетание субтитров — ⌘⌥T. Cmd+Q завершает активную сессию и сохраняет историю.

## Linux

DEB предназначен для совместимых Debian/Ubuntu, RPM — для совместимых RPM-дистрибутивов. AppImage всё равно требует подходящие glibc, графический стек, аудиосервис и Secret Service. Baseline сборок — Ubuntu 22.04; произвольный дистрибутив не гарантируется.

```sh
sudo apt install ./BabelHack-0.7.0-linux-x64.deb
# либо для подходящей RPM-системы:
sudo dnf install ./BabelHack-0.7.0-linux-x64.rpm
# либо:
chmod +x BabelHack-0.7.0-linux-x64.AppImage
./BabelHack-0.7.0-linux-x64.AppImage
```

Для ARM64 замените `linux-x64` на `linux-arm64`. В AppImage при отсутствии FUSE можно использовать `--appimage-extract-and-run`. Нужны запущенный PulseAudio или PipeWire с pipewire-pulse, обычная пользовательская D-Bus-сессия и разблокированный Secret Service (например, GNOME Keyring). Нет перехода к открытому хранению API-ключей при недоступном хранилище.

Для сборки Ubuntu/Debian:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libasound2-dev libdbus-1-dev libayatana-appindicator3-dev patchelf rpm
npm ci
npm run build:desktop
```

Backend явно выбирает PulseAudio host, находит `monitor_source_index` выбранного sink и проверяет `monitor_of_sink_index`. ALSA-вход и микрофон по умолчанию не используются.

В X11 Ctrl+Alt+T работает, если сочетание свободно. В Wayland X11 global-hotkey manager отключён, абсолютные координаты не восстанавливаются; управление доступно через интерфейс/трей. Режим поверх окон и размещение контролирует compositor. GNOME может требовать AppIndicator-расширение. На Linux закрытие крестиком по умолчанию завершает приложение; скрытие в трей включается в настройках после проверки его доступности.

## Данные и обновление

Путь профиля и namespace ключей `interview-translator` сохранены совместимыми с версиями до переименования; точные пути — в [README](../README.md#данные-и-совместимость). Новый bundle ID — `io.github.mr-lexus.babelhack`. Старый автозапуск нужно отключить отдельно. Ключи не переносятся между ОС копированием `config.json`.

## Проверки перед расширением поддержки

Ручная проверка нужна для установки/удаления, выдачи/отзыва macOS-разрешения, USB/Bluetooth duplex, отключения устройства, PipeWire, Wayland, заблокированного keyring, нескольких мониторов/DPI и длительных разговоров. Платные API-тесты запускаются только явно, на синтетических данных.

Для GTK3/Tauri используется локальный backport исправления GLib 0.18.5: [PATCHES.md](../src-tauri/vendor/glib/PATCHES.md), [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html). Linux CI выполняет регрессионный тест. OSV-скан отмечает override отдельно; это не замена ревью.
