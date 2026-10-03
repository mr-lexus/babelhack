import { useEffect, useRef, useState, type CSSProperties } from "react";
import { command, desktop, errorText, on } from "./api";
import { duration, useConfig, useSession } from "./hooks";
import { BrandMark } from "./BrandMark";
import { version } from "../package.json";
import { Icon } from "./Icon";
import Settings from "./Settings";
import { DEFAULT_PLATFORM, LANGUAGES } from "./types";
import type { MetricsPayload, SessionRecord, SessionSummary } from "./types";
import { exportRecord } from "./export";

type Page = "live" | "history" | "settings";
const statusLabels: Record<string, string> = {
  idle: "Готов к работе",
  connecting: "Подключение",
  listening: "Слушаю",
  reconnecting: "Переподключение",
  error: "Нужна проверка",
  stopping: "Завершение",
};

export default function App() {
  const { state, error: sessionError } = useSession();
  const { config, error: configError } = useConfig();
  const [page, setPage] = useState<Page>("live");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [title, setTitle] = useState("");
  const demo = state.demo;
  const [record, setRecord] = useState<SessionRecord | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [historyQuery, setHistoryQuery] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const [metrics, setMetrics] = useState<MetricsPayload | null>(null);
  const [diagnostics, setDiagnostics] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);
  const platform = config?.platform ?? DEFAULT_PLATFORM;
  const ready = !!config?.has_deepgram && !!config?.has_openai;
  const recordId = page === "history" ? selected : state.session_id;

  useEffect(() => {
    const subs = [
      on<string>("navigate", (value) => {
        if (value === "settings") setPage("settings");
      }),
      on<string>("tray-error", setError),
    ];
    return () => {
      subs.forEach((p) => void p.then((fn) => fn()).catch(() => {}));
    };
  }, []);
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  useEffect(() => {
    let disposed = false;
    let revision = 0;
    const refresh = async () => {
      const request = ++revision;
      try {
        const [list, current] = await Promise.all([
          page === "history"
            ? command<SessionSummary[]>("list_sessions")
            : Promise.resolve(null),
          recordId
            ? command<SessionRecord>("get_session", { id: recordId })
            : Promise.resolve(null),
        ]);
        if (!disposed && request === revision) {
          if (list) setSessions(list);
          setRecord(current);
        }
      } catch (e) {
        if (!disposed) setError(errorText(e));
      }
    };
    setRecord(null);
    const sub = on("history-changed", refresh);
    void sub.then(refresh).catch((e) => setError(errorText(e)));
    return () => {
      disposed = true;
      void sub.then((fn) => fn()).catch(() => {});
    };
  }, [recordId, page]);
  useEffect(() => {
    if (autoScroll && page === "live") {
      const feed = bottom.current?.parentElement;
      feed?.scrollTo({ top: feed.scrollHeight, behavior: "smooth" });
    }
  }, [record?.entries.length, autoScroll, page]);
  useEffect(() => {
    if (!diagnostics) return;
    const refresh = () =>
      command<MetricsPayload>("get_metrics")
        .then(setMetrics)
        .catch((e) => setError(errorText(e)));
    void refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [diagnostics]);
  useEffect(() => {
    if (notice) {
      const t = setTimeout(() => setNotice(""), 4500);
      return () => clearTimeout(t);
    }
  }, [notice]);

  async function act(name: string, args?: Record<string, unknown>) {
    setBusy(true);
    setError("");
    try {
      await command(name, args);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function start(isDemo: boolean) {
    setPage("live");
    setQuery("");
    await act("start_session", { title, demo: isDemo });
  }
  async function copy() {
    if (!record) return;
    try {
      await navigator.clipboard.writeText(
        record.entries
          .map(
            (e) =>
              e.source +
              "\n" +
              (e.translation || e.error || "Перевод ожидается"),
          )
          .join("\n\n"),
      );
      setNotice("Текст скопирован");
    } catch (e) {
      setError("Не удалось скопировать: " + errorText(e));
    }
  }
  async function download(format: string) {
    if (!record) return;
    setBusy(true);
    try {
      const saved = await exportRecord(record, format);
      if (saved) setNotice("Экспорт сохранён");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  const entries = (record?.entries ?? []).filter((e) =>
    (e.source + " " + e.translation)
      .toLocaleLowerCase()
      .includes(query.toLocaleLowerCase()),
  );
  const elapsed = record?.ended_at
    ? record.ended_at - record.started_at
    : state.started_at
      ? now - state.started_at
      : 0;
  const displayedError = error || sessionError || configError;
  const active = state.active;
  const titleLabel =
    page === "live"
      ? "Живой перевод"
      : page === "history"
        ? "История сессий"
        : "Настройки";

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setPage("live");
          }}
        >
          <span className="brand-mark">
            <BrandMark size={40} />
          </span>
          <span>
            interview<span className="brand-sub">TRANSLATOR</span>
          </span>
        </a>
        <div className="nav-caption">РАБОЧЕЕ ПРОСТРАНСТВО</div>
        <nav aria-label="Основная навигация">
          {(
            [
              ["live", "live", "Живой перевод"],
              ["history", "history", "История сессий"],
              ["settings", "settings", "Настройки"],
            ] as const
          ).map(([id, icon, label]) => (
            <button
              className={"nav-item " + (page === id ? "selected" : "")}
              key={id}
              aria-label={label}
              title={label}
              onClick={() => {
                setPage(id);
                setQuery("");
              }}
            >
              <Icon name={icon} />
              <span>{label}</span>
              {id === "live" && active && <i className="live-dot" />}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-note">
            <Icon name="shield" size={19} />
            <div>
              Ваша история — у вас<span>Хранится на этом компьютере</span>
            </div>
          </div>
          <div className="sidebar-version">
            <span>DESKTOP · {platform.label.toUpperCase()}</span>
            <span>v{version}</span>
          </div>
        </div>
      </aside>
      <div className="workspace">
        <nav className="mobile-nav" aria-label="Навигация на узком экране">
          {(["live", "history", "settings"] as const).map((p) => (
            <button key={p} onClick={() => setPage(p)}>
              {p === "live"
                ? "Перевод"
                : p === "history"
                  ? "История"
                  : "Настройки"}
            </button>
          ))}
        </nav>
        <header className="topbar">
          <div className="breadcrumb">
            Рабочее пространство <Icon name="chevron" size={13} />{" "}
            <strong>{titleLabel}</strong>
          </div>
          <div
            className={
              "status-badge " +
              (active && !state.paused ? "active" : "") +
              (state.status === "error" ? " danger" : "")
            }
          >
            <i />
            {state.paused
              ? "На паузе"
              : (statusLabels[state.status] ?? state.status)}
          </div>
        </header>
        <main>
          <div className="page-heading">
            <div>
              <div className="eyebrow">
                {page === "live"
                  ? "ПОНИМАТЬ КАЖДОЕ СЛОВО"
                  : page === "history"
                    ? "НИЧЕГО НЕ ТЕРЯЕТСЯ"
                    : "ВАШ РАБОЧИЙ РИТМ"}
              </div>
              <h1>{titleLabel}</h1>
              <p>
                {page === "live"
                  ? "Слушайте собеседника. Перевод всегда перед глазами."
                  : page === "history"
                    ? "Возвращайтесь к важным фразам и сохраняйте разговоры."
                    : "Подключения, языки и субтитры — всё в одном месте."}
              </p>
            </div>
            {page === "live" && (
              <button
                className="button secondary overlay-button"
                disabled={!desktop}
                onClick={() => void act("toggle_overlay")}
              >
                <Icon name="live" size={17} />
                Окно субтитров
                {!platform.wayland && <kbd>{platform.overlay_shortcut}</kbd>}
              </button>
            )}
          </div>
          {!desktop && (
            <div className="banner subtle">
              <Icon name="info" size={18} />
              <span>
                Предпросмотр в браузере. Демо работает без ключей; системный
                звук и сохранение настроек доступны в настольном приложении.
              </span>
            </div>
          )}
          {displayedError && (
            <div className="banner error" role="alert">
              <Icon name="info" size={18} />
              <span>{displayedError}</span>
              <button aria-label="Скрыть ошибку" onClick={() => setError("")}>
                <Icon name="close" size={16} />
              </button>
            </div>
          )}
          {state.message && page === "live" && (
            <div
              className={
                "banner " + (state.status === "error" ? "error" : "subtle")
              }
              role="status"
            >
              <Icon name="info" size={18} />
              <span>{state.message}</span>
            </div>
          )}
          <div hidden={page !== "settings"}>
            <Settings
              config={config}
              active={active}
              onSaved={() => setNotice("Настройки сохранены")}
            />
          </div>
          {page === "live" && (
            <>
              {!ready && !active && (
                <section className="setup-banner">
                  <div className="setup-icon">
                    <Icon name="globe" size={24} />
                  </div>
                  <div>
                    <strong>Начнём с подключения</strong>
                    <p>
                      Добавьте ключи Deepgram и OpenAI — или сначала попробуйте
                      демо.
                    </p>
                  </div>
                  <button
                    className="button secondary"
                    onClick={() => setPage("settings")}
                  >
                    Настроить
                    <Icon name="arrow" size={16} />
                  </button>
                </section>
              )}
              <section className="session-panel">
                <div className="session-options">
                  <label className="session-name">
                    НАЗВАНИЕ СЕССИИ
                    <input
                      value={title}
                      onChange={(e) => setTitle(e.target.value)}
                      maxLength={120}
                      placeholder="Например, интервью с командой"
                      disabled={active}
                    />
                  </label>
                  <div className="language-pair">
                    <span className="lang-symbol">
                      <Icon name="globe" size={20} />
                    </span>
                    <div>
                      <small>ЯЗЫК РЕЧИ</small>
                      <strong>
                        {
                          LANGUAGES[
                            active
                              ? (record?.source_language ??
                                (demo
                                  ? "en"
                                  : (config?.source_language ?? "en")))
                              : (config?.source_language ?? "en")
                          ]
                        }
                      </strong>
                    </div>
                    <Icon name="arrow" size={18} />
                    <div>
                      <small>ПЕРЕВОД</small>
                      <strong>
                        {
                          LANGUAGES[
                            active
                              ? (record?.target_language ??
                                (demo
                                  ? "ru"
                                  : (config?.target_language ?? "ru")))
                              : (config?.target_language ?? "ru")
                          ]
                        }
                      </strong>
                    </div>
                    <button
                      className="icon-button"
                      title="Изменить языки"
                      aria-label="Изменить языки"
                      disabled={active}
                      onClick={() => setPage("settings")}
                    >
                      <Icon name="settings" size={17} />
                    </button>
                  </div>
                </div>
                <div className="session-control">
                  <div className="audio-info">
                    <Icon name="volume" size={19} />
                    <div>
                      <strong>
                        {demo && active
                          ? "Демонстрационная сессия"
                          : "Системный звук"}
                      </strong>
                      <span
                        title={
                          config?.audio_device_name ??
                          config?.audio_device ??
                          ""
                        }
                      >
                        {demo && active
                          ? "Пример EN → RU · без API-запросов"
                          : config?.audio_device_name ||
                            config?.audio_device ||
                            "Устройство воспроизведения по умолчанию"}
                      </span>
                    </div>
                  </div>
                  <div
                    className="audio-meter"
                    aria-label="Уровень звука"
                    role="meter"
                    aria-valuenow={Math.round(state.audio_level * 100)}
                    aria-valuemin={0}
                    aria-valuemax={100}
                  >
                    {Array.from({ length: 16 }, (_, i) => (
                      <i
                        key={i}
                        className={
                          !state.paused &&
                          active &&
                          i < Math.sqrt(state.audio_level) * 30
                            ? "lit"
                            : ""
                        }
                      />
                    ))}
                  </div>
                  <div className="control-buttons">
                    {active ? (
                      <>
                        <span className="timer">{duration(elapsed)}</span>
                        <button
                          className="button secondary"
                          disabled={busy || state.status === "stopping"}
                          onClick={() =>
                            void act(
                              state.paused ? "resume_session" : "pause_session",
                            )
                          }
                        >
                          <Icon
                            name={state.paused ? "play" : "pause"}
                            size={16}
                          />
                          {state.paused ? "Продолжить" : "Пауза"}
                        </button>
                        <button
                          className="button stop"
                          disabled={busy}
                          onClick={() => void act("stop_session")}
                        >
                          <Icon name="stop" size={16} />
                          {busy ? "Подождите…" : "Завершить"}
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          className="button ghost"
                          disabled={busy || !config}
                          onClick={() => void start(true)}
                        >
                          <Icon name="play" size={16} />
                          Демо
                        </button>
                        <button
                          className="button primary"
                          disabled={busy || !ready || !desktop}
                          onClick={() => void start(false)}
                        >
                          <Icon name="play" size={17} />
                          {busy ? "Подключение…" : "Начать перевод"}
                        </button>
                      </>
                    )}
                  </div>
                </div>
              </section>
              <section className="live-cards">
                <div className="live-card source-card">
                  <div className="card-label">
                    <span>
                      <span className="language-code">
                        {(active
                          ? state.source_language
                          : (config?.source_language ?? "en")
                        ).toUpperCase()}
                      </span>
                      Речь собеседника
                    </span>
                    {active && !state.paused && (
                      <span className="tiny-live">LIVE</span>
                    )}
                  </div>
                  <p className={state.source ? "" : "placeholder"}>
                    {state.source ||
                      (state.paused
                        ? "Захват звука на паузе"
                        : active
                          ? "Ожидаю речь собеседника…"
                          : "Здесь появится распознанная речь")}
                  </p>
                  <div className="card-foot">
                    <Icon name="wave" size={15} />
                    {active
                      ? "Распознавание в реальном времени"
                      : "Аудио из Zoom, Teams, браузера и других приложений"}
                  </div>
                </div>
                <div className="live-card translated-card">
                  <div className="card-label">
                    <span>
                      <span className="language-code">
                        {(active
                          ? state.target_language
                          : (config?.target_language ?? "ru")
                        ).toUpperCase()}
                      </span>
                      Перевод
                    </span>
                    <span className="translated-label">
                      <Icon name="globe" size={13} /> AI
                    </span>
                  </div>
                  <p
                    className={
                      state.current || state.lines.length ? "" : "placeholder"
                    }
                  >
                    {state.current ||
                      state.lines.at(-1) ||
                      (active
                        ? "Перевод появится после первой фразы…"
                        : "Смысл разговора — на вашем языке")}
                    {state.current && <span className="typing-cursor" />}
                  </p>
                  <div className="card-foot">
                    <Icon name="check" size={15} />
                    {state.pending > 0
                      ? "Фраз в обработке: " + state.pending
                      : "Законченные фразы сохраняются в ленте ниже"}
                  </div>
                </div>
              </section>
            </>
          )}
          {page === "history" && (
            <section className="history-list">
              <div className="search">
                <Icon name="search" size={17} />
                <input
                  aria-label="Найти сессию"
                  placeholder="Найти сессию по названию…"
                  value={historyQuery}
                  onChange={(e) => setHistoryQuery(e.target.value)}
                />
              </div>
              {sessions.length === 0 ? (
                <div className="empty-state">
                  <span className="empty-icon">
                    <Icon name="history" size={30} />
                  </span>
                  <h3>Здесь будут ваши разговоры</h3>
                  <p>
                    Завершённые сессии сохраняются автоматически,
                    <br />
                    если история включена в настройках.
                  </p>
                  <button
                    className="button secondary"
                    onClick={() => setPage("live")}
                  >
                    Перейти к переводу
                    <Icon name="arrow" size={16} />
                  </button>
                </div>
              ) : (
                <div className="session-list">
                  {sessions
                    .filter((s) =>
                      s.title
                        .toLowerCase()
                        .includes(historyQuery.toLowerCase()),
                    )
                    .map((s) => (
                      <div
                        className={
                          "session-row " + (selected === s.id ? "selected" : "")
                        }
                        key={s.id}
                      >
                        <button onClick={() => setSelected(s.id)}>
                          <span className="session-row-icon">
                            <Icon name="history" size={20} />
                          </span>
                          <span>
                            <strong>{s.title}</strong>
                            <small>
                              {new Date(s.started_at).toLocaleString("ru-RU", {
                                day: "numeric",
                                month: "long",
                                hour: "2-digit",
                                minute: "2-digit",
                              })}{" "}
                              · {s.entry_count} фраз
                            </small>
                          </span>
                          <span className="row-languages">
                            {s.source_language.toUpperCase()} →{" "}
                            {s.target_language.toUpperCase()}
                          </span>
                          <Icon name="chevron" size={16} />
                        </button>
                        <button
                          className="icon-button"
                          aria-label={"Удалить сессию " + s.title}
                          disabled={active && state.session_id === s.id}
                          onClick={() => setDeleteId(s.id)}
                        >
                          <Icon name="trash" size={16} />
                        </button>
                      </div>
                    ))}
                </div>
              )}
            </section>
          )}
          {(page === "live" || (page === "history" && record)) && (
            <section className="transcript-panel">
              <div className="transcript-header">
                <div className="section-title">
                  <Icon name="history" size={18} />
                  <h2>
                    {page === "history" ? record?.title : "Лента разговора"}
                  </h2>
                  <span className="count">{record?.entries.length ?? 0}</span>
                </div>
                <div className="transcript-actions">
                  {page === "live" && (
                    <label className="checkbox-label">
                      <input
                        type="checkbox"
                        checked={autoScroll}
                        onChange={(e) => setAutoScroll(e.target.checked)}
                      />
                      Автопрокрутка
                    </label>
                  )}
                  <button
                    className="icon-button"
                    aria-label="Копировать разговор"
                    disabled={!record?.entries.length}
                    onClick={() => void copy()}
                  >
                    <Icon name="copy" size={17} />
                  </button>
                  <details className="export-menu">
                    <summary
                      aria-disabled={!record?.entries.length}
                      onClick={(e) => {
                        if (!record?.entries.length) e.preventDefault();
                      }}
                    >
                      <Icon name="download" size={17} />
                      <span>Экспорт</span>
                    </summary>
                    <div>
                      {["txt", "md", "json"].map((f) => (
                        <button
                          key={f}
                          disabled={!record?.entries.length || busy}
                          onClick={(e) => {
                            e.currentTarget
                              .closest("details")
                              ?.removeAttribute("open");
                            void download(f);
                          }}
                        >
                          {f.toUpperCase()}
                        </button>
                      ))}
                    </div>
                  </details>
                </div>
              </div>
              {page === "live" && !!record?.entries.length && (
                <div className="search transcript-search">
                  <Icon name="search" size={16} />
                  <input
                    aria-label="Поиск по разговору"
                    placeholder="Поиск по оригиналу и переводу…"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </div>
              )}
              {!record?.entries.length ? (
                <div className="empty-state transcript-empty">
                  <div className="empty-wave">
                    {[10, 20, 31, 45, 28, 50, 35, 22, 12].map((h, i) => (
                      <i style={{ height: h }} key={i} />
                    ))}
                  </div>
                  <h3>
                    {active ? "Разговор начинается" : "Место для важных слов"}
                  </h3>
                  <p>
                    {active
                      ? "Включите звук на выбранном устройстве. Готовые фразы появятся здесь."
                      : "Начните перевод или запустите демо,\nчтобы увидеть, как всё работает."}
                  </p>
                  {!active && (
                    <button
                      className="text-button"
                      disabled={busy || !config}
                      onClick={() => void start(true)}
                    >
                      Попробовать демо
                      <Icon name="arrow" size={16} />
                    </button>
                  )}
                </div>
              ) : (
                <div
                  className="transcript-scroll"
                  style={
                    {
                      "--history-font-size":
                        (config?.history_font_size ?? 13) + "px",
                      "--history-line-height":
                        config?.history_line_height ?? 1.25,
                      "--history-entry-spacing":
                        (config?.history_entry_spacing ?? 8) + "px",
                    } as CSSProperties
                  }
                >
                  {entries.length === 0 && (
                    <div className="no-results">
                      Ничего не найдено. Попробуйте другое слово.
                    </div>
                  )}
                  {entries.map((e) => (
                    <article className="transcript-entry" key={e.id}>
                      <time>{duration(e.timestamp_ms)}</time>
                      <div>
                        <p className="entry-source">{e.source}</p>
                        <p
                          className={
                            "entry-translation " +
                            (e.error ? "entry-error" : "")
                          }
                        >
                          {e.translation || e.error || "Перевожу…"}
                        </p>
                      </div>
                    </article>
                  ))}
                  <div ref={bottom} />
                </div>
              )}
              <div className="transcript-footer">
                <span>
                  <Icon name="shield" size={14} />
                  {demo && state.session_id === record?.id
                    ? "Демо не сохраняется на диск"
                    : (record ? state.save_history : config?.save_history)
                      ? "История сохраняется локально"
                      : "Сохранение истории выключено"}
                </span>
                <span>
                  {active
                    ? "Аудио → Deepgram · Текст → OpenAI"
                    : "Аудиозапись на диск не ведётся"}
                </span>
              </div>
            </section>
          )}
          {page === "live" && (
            <>
              <button
                className="diagnostics-toggle"
                onClick={() => setDiagnostics((v) => !v)}
              >
                <Icon name="info" size={15} />
                {diagnostics ? "Скрыть диагностику" : "Диагностика подключения"}
                <Icon name="chevron" size={13} />
              </button>
              {diagnostics && (
                <section className="diagnostics">
                  <div>
                    <small>ПЕРВЫЙ ТОКЕН</small>
                    <strong>
                      {metrics?.last_ttft == null
                        ? "—"
                        : Math.round(metrics.last_ttft) + " мс"}
                    </strong>
                  </div>
                  <div>
                    <small>ПЕРЕВОД · P95</small>
                    <strong>
                      {metrics?.total.count
                        ? Math.round(metrics.total.p95) + " мс"
                        : "—"}
                    </strong>
                  </div>
                  <div>
                    <small>ОЧЕРЕДЬ ФРАЗ</small>
                    <strong>{state.pending}</strong>
                  </div>
                  <div>
                    <small>ПОТЕРЯНО АУДИОПАКЕТОВ</small>
                    <strong>{state.dropped_chunks}</strong>
                  </div>
                  <p>
                    При отсутствии речи проверьте выбранное устройство
                    воспроизведения и громкость приложения. Пауза останавливает
                    захват; уже распознанные фразы могут завершить перевод.
                  </p>
                </section>
              )}
            </>
          )}
          <footer className="page-footer">
            <span>Babel Hack</span>
            <span>Меньше языковых барьеров. Больше понимания.</span>
          </footer>
        </main>
      </div>
      {notice && (
        <div className="toast" role="status">
          <Icon name="check" size={18} />
          {notice}
        </div>
      )}
      {deleteId && (
        <div className="modal-backdrop">
          <section
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-title"
            className="confirm-dialog"
          >
            <h2 id="delete-title">Удалить эту сессию?</h2>
            <p>
              Локальная история этого разговора будет удалена. Экспортированные
              файлы сохранятся.
            </p>
            <div>
              <button
                autoFocus
                className="button secondary"
                onClick={() => setDeleteId(null)}
              >
                Отмена
              </button>
              <button
                className="button stop"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  try {
                    await command("delete_session", { id: deleteId });
                    setSessions((s) => s.filter((v) => v.id !== deleteId));
                    if (selected === deleteId) {
                      setSelected(null);
                      setRecord(null);
                    }
                    setDeleteId(null);
                  } catch (e) {
                    setError(errorText(e));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Удалить
              </button>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
