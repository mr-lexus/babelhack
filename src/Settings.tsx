import { useEffect, useState, type CSSProperties } from "react";
import { command, desktop, errorText } from "./api";
import { Icon } from "./Icon";
import { DEFAULT_CONFIG, DEFAULT_PLATFORM, LANGUAGES } from "./types";
import type { ConfigStatus, OutputDevice } from "./types";

export default function Settings({
  config,
  active,
  onSaved,
}: {
  config: ConfigStatus | null;
  active: boolean;
  onSaved: () => void;
}) {
  const [draft, setDraft] = useState<ConfigStatus>(config ?? DEFAULT_CONFIG);
  const [keys, setKeys] = useState({ deepgram: "", openai: "" });
  const [devices, setDevices] = useState<OutputDevice[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [dirty, setDirty] = useState(false);
  const platform = config?.platform ?? DEFAULT_PLATFORM;
  const selectedDevice = devices.find(
    (d) => d.id === draft.audio_device || d.name === draft.audio_device,
  );
  const [autostart, setAutostart] = useState(false);
  useEffect(() => {
    if (config && !dirty) setDraft(config);
  }, [config, dirty]);
  useEffect(() => {
    void command<OutputDevice[]>("list_output_devices")
      .then(setDevices)
      .catch((e) => setError(errorText(e)));
    if (desktop)
      void import("@tauri-apps/plugin-autostart")
        .then((p) => p.isEnabled())
        .then(setAutostart)
        .catch((e) => setError(errorText(e)));
  }, []);
  function field<K extends keyof ConfigStatus>(key: K, value: ConfigStatus[K]) {
    setDraft((d) => ({ ...d, [key]: value }));
    setDirty(true);
  }
  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await command("set_config", {
        args: {
          config: draft,
          deepgramKey: keys.deepgram || null,
          openaiKey: keys.openai || null,
        },
      });
      setKeys({ deepgram: "", openai: "" });
      setDirty(false);
      onSaved();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form className="settings-layout" onSubmit={save}>
      <div className="settings-main">
        {active && (
          <div className="banner subtle">
            <Icon name="info" size={18} />
            <span>
              Языки, модель, устройство и сохранение истории применятся к
              следующей сессии. Оформление субтитров и истории изменится после
              сохранения.
            </span>
          </div>
        )}
        {config?.credential_error && (
          <div className="banner error" role="alert">
            {config.credential_error}
          </div>
        )}
        {error && (
          <div className="banner error" role="alert">
            {error}
          </div>
        )}
        <section className="settings-section">
          <div className="settings-section-heading">
            <span className="setting-icon">
              <Icon name="shield" />
            </span>
            <div>
              <h2>Подключения</h2>
              <p>Ваши ключи, ваши аккаунты. Оплата API — у провайдеров.</p>
            </div>
          </div>
          <div className="field-grid">
            <label className="field">
              <span>
                Deepgram API key{" "}
                <b className={config?.has_deepgram ? "key-ok" : "key-missing"}>
                  {config?.has_deepgram ? "Сохранён" : "Не задан"}
                </b>
              </span>
              <input
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={keys.deepgram}
                disabled={!desktop}
                placeholder={
                  config?.has_deepgram
                    ? "Оставьте пустым, чтобы сохранить ключ"
                    : "Вставьте ключ Deepgram"
                }
                onChange={(e) => {
                  setKeys((k) => ({ ...k, deepgram: e.target.value }));
                  setDirty(true);
                }}
              />
              <small>Распознаёт системный звук в текст.</small>
            </label>
            <label className="field">
              <span>
                OpenAI API key{" "}
                <b className={config?.has_openai ? "key-ok" : "key-missing"}>
                  {config?.has_openai ? "Сохранён" : "Не задан"}
                </b>
              </span>
              <input
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={keys.openai}
                disabled={!desktop}
                placeholder={
                  config?.has_openai
                    ? "Оставьте пустым, чтобы сохранить ключ"
                    : "Вставьте ключ OpenAI"
                }
                onChange={(e) => {
                  setKeys((k) => ({ ...k, openai: e.target.value }));
                  setDirty(true);
                }}
              />
              <small>Переводит распознанные фразы.</small>
            </label>
          </div>
          <div className="inline-note">
            <Icon name="shield" size={15} />
            Ключи хранятся в {platform.credential_store} и не возвращаются в
            интерфейс.
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-section-heading">
            <span className="setting-icon">
              <Icon name="globe" />
            </span>
            <div>
              <h2>Языки и перевод</h2>
              <p>Настройте перевод под свой разговор.</p>
            </div>
          </div>
          <div className="field-grid">
            <label className="field">
              <span>Язык собеседника</span>
              <select
                aria-label="Язык собеседника"
                value={draft.source_language}
                onChange={(e) => field("source_language", e.target.value)}
              >
                {Object.entries(LANGUAGES).map(([code, name]) => (
                  <option key={code} value={code}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Переводить на</span>
              <select
                aria-label="Переводить на"
                value={draft.target_language}
                onChange={(e) => field("target_language", e.target.value)}
              >
                {Object.entries(LANGUAGES).map(([code, name]) => (
                  <option key={code} value={code}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>Модель перевода</span>
              <input
                value={draft.openai_model}
                list="models"
                maxLength={120}
                required
                onChange={(e) => field("openai_model", e.target.value)}
              />
              <datalist id="models">
                <option value="gpt-4.1-mini" />
                <option value="gpt-4.1-nano" />
                <option value="gpt-5-nano" />
              </datalist>
              <small>
                Модель должна поддерживать Chat Completions и потоковый ответ.
              </small>
            </label>
            <label className="field">
              <span>Модель распознавания</span>
              <select
                value={draft.deepgram_model}
                onChange={(e) => field("deepgram_model", e.target.value)}
              >
                <option value="nova-3">Deepgram Nova-3</option>
                <option value="nova-2">Deepgram Nova-2</option>
              </select>
              <small>
                Доступность языка зависит от модели и аккаунта Deepgram.
              </small>
            </label>
          </div>
          <label className="field glossary">
            <span>
              Словарь перевода <small>{draft.glossary.length} / 4000</small>
            </span>
            <textarea
              aria-label="Словарь перевода"
              rows={4}
              maxLength={4000}
              placeholder={
                "pull request = пул-реквест\non-call = дежурство\nAcme = Acme"
              }
              value={draft.glossary}
              onChange={(e) => field("glossary", e.target.value)}
            />
            <small>
              Термин = предпочтительный перевод, по одному на строку. Передаётся
              переводчику как справочная информация.
            </small>
          </label>
        </section>
        <section className="settings-section">
          <div className="settings-section-heading">
            <span className="setting-icon">
              <Icon name="volume" />
            </span>
            <div>
              <h2>Звук и хранение</h2>
              <p>Захватывается звук выбранного устройства воспроизведения.</p>
            </div>
          </div>
          <label className="field">
            <span>
              Аудиоустройство{" "}
              <button
                type="button"
                className="text-button"
                aria-label="Обновить список устройств"
                onClick={() =>
                  void command<OutputDevice[]>("list_output_devices")
                    .then(setDevices)
                    .catch((e) => setError(errorText(e)))
                }
              >
                Обновить список
              </button>
            </span>
            <select
              aria-label="Аудиоустройство"
              value={selectedDevice?.id ?? draft.audio_device ?? ""}
              onChange={(e) => {
                field("audio_device", e.target.value || null);
                field(
                  "audio_device_name",
                  devices.find((d) => d.id === e.target.value)?.name ?? null,
                );
              }}
            >
              <option value="">Системное устройство по умолчанию</option>
              {draft.audio_device && !selectedDevice && (
                <option value={draft.audio_device}>
                  {draft.audio_device_name ?? draft.audio_device} — недоступно
                </option>
              )}
              {devices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
            <small>
              {platform.audio_backend}. {platform.audio_help}
            </small>
          </label>
          <label className="toggle-row">
            <div>
              <strong>Сохранять историю на компьютере</strong>
              <small>
                Оригинал и перевод сохраняются после каждой законченной фразы.
              </small>
            </div>
            <input
              type="checkbox"
              checked={draft.save_history}
              onChange={(e) => field("save_history", e.target.checked)}
            />
          </label>
          <label className="toggle-row">
            <div>
              <strong>Запускать при входе в систему</strong>
              <small>
                Приложение откроется при входе. Захват звука запускается
                вручную.
              </small>
            </div>
            <input
              type="checkbox"
              checked={autostart}
              disabled={!desktop || busy}
              onChange={async (e) => {
                const checked = e.target.checked;
                setBusy(true);
                try {
                  const p = await import("@tauri-apps/plugin-autostart");
                  await (checked ? p.enable() : p.disable());
                  setAutostart(checked);
                } catch (e) {
                  setError(errorText(e));
                } finally {
                  setBusy(false);
                }
              }}
            />
          </label>
          <div className="inline-note">
            <Icon name="info" size={15} />
            При переводе звук отправляется в Deepgram, а текст — в OpenAI.
            Аудиофайлы локально не сохраняются.
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-section-heading">
            <div>
              <h2>Окно и системный трей</h2>
              <p>Перевод продолжает работать, когда главное окно скрыто.</p>
            </div>
          </div>
          {desktop && !config?.tray_available && (
            <div className="inline-note">
              Системный трей недоступен. Окно сворачивается и закрывается
              обычным способом.
            </div>
          )}
          <label className="toggle-row">
            <div>
              <strong>Закрывать крестиком в трей</strong>
              <small>
                Крестик и Alt+F4 скрывают окно. Для полного выхода выберите
                «Выйти из приложения» в меню трея.
              </small>
            </div>
            <input
              type="checkbox"
              checked={draft.close_to_tray ?? true}
              disabled={desktop && !config?.tray_available}
              onChange={(e) => field("close_to_tray", e.target.checked)}
            />
          </label>
          <label className="toggle-row">
            <div>
              <strong>Сворачивать в трей вместо панели задач</strong>
              <small>
                Кнопка «Свернуть» скрывает главное окно и убирает его из панели
                задач. Субтитры остаются видимыми.
              </small>
            </div>
            <input
              type="checkbox"
              checked={draft.minimize_to_tray ?? false}
              disabled={desktop && !config?.tray_available}
              onChange={(e) => field("minimize_to_tray", e.target.checked)}
            />
          </label>
          <div className="inline-note">
            {platform.os === "linux"
              ? "Для трея Linux нужна поддержка AppIndicator в рабочем окружении (например, расширение GNOME). Вернуть окно можно пунктом «Показать окно» в меню значка."
              : "Меню значка: показать окно, субтитры, начать / приостановить / продолжить перевод, завершить сессию, настройки и выход."}
          </div>
        </section>
        <section className="settings-section">
          <div className="settings-section-heading">
            <span className="setting-icon">
              <Icon name="history" />
            </span>
            <div>
              <h2>Лента и сохранённая история</h2>
              <p>Оформление записей в основном окне</p>
            </div>
          </div>
          <div
            className="history-style-preview"
            style={
              {
                "--history-font-size": (draft.history_font_size ?? 13) + "px",
                "--history-line-height": draft.history_line_height ?? 1.25,
                "--history-entry-spacing":
                  (draft.history_entry_spacing ?? 8) + "px",
              } as CSSProperties
            }
          >
            <article className="history-preview-entry">
              <time>00:12</time>
              <div>
                <p className="entry-source">Tell me about your experience.</p>
                <p className="entry-translation">Расскажите о своём опыте.</p>
              </div>
            </article>
            <article className="history-preview-entry">
              <time>00:18</time>
              <div>
                <p className="entry-source">
                  I enjoy building reliable products.
                </p>
                <p className="entry-translation">
                  Мне нравится создавать надёжные продукты.
                </p>
              </div>
            </article>
          </div>
          {(
            [
              [
                "history_font_size",
                "Шрифт ленты и сохранённой истории",
                11,
                24,
                1,
                13,
              ],
              [
                "history_line_height",
                "Интервал строк ленты и сохранённой истории",
                1,
                2,
                0.05,
                1.25,
              ],
              [
                "history_entry_spacing",
                "Отступы записей в ленте и истории",
                2,
                24,
                1,
                8,
              ],
            ] as const
          ).map(([key, label, min, max, step, fallback]) => (
            <label className="range-field" key={key}>
              <span>
                {label}{" "}
                <b>
                  {step < 1
                    ? (draft[key] ?? fallback).toFixed(2)
                    : (draft[key] ?? fallback) + "px"}
                </b>
              </span>
              <input
                type="range"
                min={min}
                max={max}
                step={step}
                value={draft[key] ?? fallback}
                onChange={(e) => field(key, Number(e.target.value))}
              />
            </label>
          ))}
          <button
            type="button"
            className="button secondary"
            onClick={() => {
              setDraft((d) => ({
                ...d,
                history_font_size: 13,
                history_line_height: 1.25,
                history_entry_spacing: 8,
              }));
              setDirty(true);
            }}
          >
            Компактный вид истории
          </button>
        </section>
      </div>
      <aside className="settings-preview">
        <section className="settings-section">
          <div className="settings-section-heading">
            <span className="setting-icon">
              <Icon name="live" />
            </span>
            <div>
              <h2>Субтитры</h2>
              <p>Поверх остальных окон</p>
            </div>
          </div>
          {!platform.overlay_transparency && (
            <p className="inline-note">
              На macOS фон непрозрачный; цвет и размеры настраиваются.
            </p>
          )}
          <div
            className="overlay-preview"
            style={{
              background: draft.overlay_bg_color,
              boxShadow: "inset 0 0 0 1px #ffffff10",
            }}
          >
            <div className="preview-dots">
              <i />
              <i />
              <i />
              <span>ПРЕДПРОСМОТР</span>
            </div>
            <div
              className="overlay-history-preview"
              style={{
                fontSize: draft.overlay_font_size,
                lineHeight: draft.overlay_history_line_height ?? 1.2,
                color: draft.overlay_history_color,
              }}
            >
              <div style={{ marginBottom: draft.overlay_history_spacing ?? 6 }}>
                Спасибо, что присоединились к разговору.
              </div>
              <div style={{ marginBottom: draft.overlay_history_spacing ?? 6 }}>
                Обсудим ваш опыт и последние проекты.
              </div>
            </div>
            {draft.overlay_show_source && (
              <p
                className="preview-source"
                style={{ color: draft.overlay_history_color }}
              >
                Let's build something great together.
              </p>
            )}
            <p
              style={{
                color: draft.overlay_text_color,
                fontSize: draft.overlay_realtime_font_size,
                lineHeight: draft.overlay_line_height ?? 1.25,
              }}
            >
              Давайте вместе создадим что-то отличное.
            </p>
          </div>
          <label className="range-field">
            <span>
              Размер перевода <b>{draft.overlay_realtime_font_size}px</b>
            </span>
            <input
              type="range"
              min="14"
              max="64"
              step="1"
              value={draft.overlay_realtime_font_size}
              onChange={(e) =>
                field("overlay_realtime_font_size", Number(e.target.value))
              }
            />
          </label>
          <label className="range-field">
            <span>
              Межстрочный интервал перевода{" "}
              <b>{(draft.overlay_line_height ?? 1.25).toFixed(2)}</b>
            </span>
            <input
              type="range"
              min="1"
              max="2"
              step="0.05"
              value={draft.overlay_line_height ?? 1.25}
              onChange={(e) =>
                field("overlay_line_height", Number(e.target.value))
              }
            />
          </label>
          <h3 className="settings-subheading">История в оверлее</h3>
          <label className="range-field">
            <span>
              Шрифт истории в оверлее <b>{draft.overlay_font_size}px</b>
            </span>
            <input
              type="range"
              min="12"
              max="48"
              step="1"
              value={draft.overlay_font_size}
              onChange={(e) =>
                field("overlay_font_size", Number(e.target.value))
              }
            />
          </label>
          {(
            [
              [
                "overlay_history_line_height",
                "Интервал строк истории в оверлее",
                1,
                2,
                0.05,
                1.2,
              ],
              [
                "overlay_history_spacing",
                "Отступ между фразами в оверлее",
                0,
                24,
                1,
                6,
              ],
            ] as const
          ).map(([key, label, min, max, step, fallback]) => (
            <label className="range-field" key={key}>
              <span>
                {label}{" "}
                <b>
                  {step < 1
                    ? (draft[key] ?? fallback).toFixed(2)
                    : (draft[key] ?? fallback) + "px"}
                </b>
              </span>
              <input
                type="range"
                min={min}
                max={max}
                step={step}
                value={draft[key] ?? fallback}
                onChange={(e) => field(key, Number(e.target.value))}
              />
            </label>
          ))}
          <button
            type="button"
            className="button secondary full-width"
            onClick={() => {
              setDraft((d) => ({
                ...d,
                overlay_font_size: 16,
                overlay_history_line_height: 1.2,
                overlay_history_spacing: 6,
              }));
              setDirty(true);
            }}
          >
            Компактная история в оверлее
          </button>
          <h3 className="settings-subheading">Оформление окна</h3>
          <label className="range-field">
            <span>
              Непрозрачность фона{" "}
              <b>
                {Math.round(
                  (platform.overlay_transparency ? draft.overlay_opacity : 1) *
                    100,
                )}
                %
              </b>
            </span>
            <input
              type="range"
              min=".15"
              disabled={!platform.overlay_transparency}
              max="1"
              step=".01"
              value={draft.overlay_opacity}
              onChange={(e) => field("overlay_opacity", Number(e.target.value))}
            />
          </label>
          <label className="range-field">
            <span>
              Последних фраз <b>{draft.overlay_lines}</b>
            </span>
            <input
              type="range"
              min="1"
              max="8"
              step="1"
              value={draft.overlay_lines}
              onChange={(e) => field("overlay_lines", Number(e.target.value))}
            />
          </label>
          <div className="color-fields">
            <label>
              Фон
              <input
                type="color"
                value={draft.overlay_bg_color}
                onChange={(e) => field("overlay_bg_color", e.target.value)}
              />
            </label>
            <label>
              Перевод
              <input
                type="color"
                value={draft.overlay_text_color}
                onChange={(e) => field("overlay_text_color", e.target.value)}
              />
            </label>
            <label>
              История
              <input
                type="color"
                value={draft.overlay_history_color}
                onChange={(e) => field("overlay_history_color", e.target.value)}
              />
            </label>
          </div>
          <label className="toggle-row">
            <strong>Показывать оригинал</strong>
            <input
              type="checkbox"
              checked={draft.overlay_show_source}
              onChange={(e) => field("overlay_show_source", e.target.checked)}
            />
          </label>
          <button
            type="button"
            className="button secondary full-width"
            disabled={!desktop}
            onClick={() =>
              void command("reset_overlay").catch((e) => setError(errorText(e)))
            }
          >
            Вернуть окно в центр
          </button>
          <p className="hint">
            Перетащите окно за верхнюю панель. Размер меняется за правый нижний
            угол.{" "}
            {!platform.wayland && (
              <>
                <kbd>{platform.overlay_shortcut}</kbd> скрывает и показывает
                субтитры.
              </>
            )}
            {platform.wayland &&
              " В Wayland положение, режим поверх окон и глобальные сочетания определяет оконный менеджер. Используйте кнопку в главном окне."}
          </p>
        </section>
      </aside>
      <div className="save-bar">
        <span>
          {dirty ? "Есть несохранённые изменения" : "Настройки актуальны"}
        </span>
        <button
          type="submit"
          className="button primary"
          disabled={busy || !config || !dirty}
        >
          <Icon name="check" size={17} />
          {busy ? "Сохранение…" : "Сохранить настройки"}
        </button>
      </div>
    </form>
  );
}
