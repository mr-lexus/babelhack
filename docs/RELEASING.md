# Выпуск Babel Hack

1. Обновите версию в package.json, package-lock.json (root и packages[""]), src-tauri/Cargo.toml, Cargo.lock и tauri.conf.json. Обновите CHANGELOG.md и docs/releases/<version>.md.
2. Отправьте изменения в main. Workflow Desktop builds проверяет интерфейс и пять нативных целей; каждый пакет собирается на своей ОС/архитектуре.
3. Запустите workflow вручную с `release=true`: `gh workflow run desktop.yml --ref main -f release=true`.
4. Дождитесь успешных frontend и desktop jobs. Release job проверит ровно 12 пакетов, сформирует SHA256SUMS.txt и создаст **draft** с тегом v<version> на проверенном commit. Повторное создание существующего тега/релиза не перезаписывает опубликованные файлы: для исправления выпустите новую версию.
5. Проверьте assets, notes, результаты CI и доступные ручные smoke-проверки. Опубликуйте: `gh release edit v<version> --draft=false --latest`.

Права contents:write есть только у release job; сборки используют read. PR не может создать релиз. Workflow использует стандартный GITHUB_TOKEN, личный PAT в репозитории не нужен. Репозиторий публичный; Linux ARM64 собирается на нативном GitHub runner.

Windows-пакеты пока без Authenticode. macOS использует ad-hoc подпись; `codesign --verify` проверяет целостность, `lipo` — архитектуру. Для Developer ID/notarization нужны отдельные сертификаты и секреты владельца; их нельзя коммитить. Перед таким выпуском обновите конфигурацию подписи и инструкции для пользователей.

Тестовые профили, API-ключи, аудио, target, node_modules и локальные artifacts исключены из Git. В release попадают только файлы, явно собранные scripts/collect-release.py и проверенные scripts/verify-release.py.
