import { useEffect, useState, type PropsWithChildren } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { command, desktop, errorText, on } from "./api";
import { useConfig } from "./hooks";
import { BrandMark } from "./BrandMark";

const edges = [
  "North",
  "South",
  "East",
  "West",
  "NorthEast",
  "NorthWest",
  "SouthEast",
  "SouthWest",
] as const;

export default function DesktopFrame({ children }: PropsWithChildren) {
  const { config } = useConfig();
  const closeToTray = config?.tray_available && config.close_to_tray;
  const [maximized, setMaximized] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [focused, setFocused] = useState(true);
  const [closing, setClosing] = useState(false);
  const [error, setError] = useState("");
  const mac = /Mac/.test(navigator.platform);
  useEffect(() => {
    if (!desktop) return;
    const win = getCurrentWindow();
    let disposed = false;
    let revision = 0;
    const refresh = async () => {
      const request = ++revision;
      try {
        const [max, full] = await Promise.all([
          win.isMaximized(),
          win.isFullscreen(),
        ]);
        if (!disposed && request === revision) {
          setMaximized(max);
          setFullscreen(full);
        }
      } catch (e) {
        if (!disposed) setError(errorText(e));
      }
    };
    void refresh();
    void win
      .isFocused()
      .then((value) => {
        if (!disposed) setFocused(value);
      })
      .catch(() => {});
    const subscriptions = [
      on("app-exiting", () => setClosing(true)),
      win.onResized(refresh),
      win.onFocusChanged(({ payload }) => {
        if (!disposed) setFocused(payload);
      }),
    ];
    return () => {
      disposed = true;
      subscriptions.forEach((p) => void p.then((f) => f()).catch(() => {}));
    };
  }, []);
  const run = async (action: "minimize" | "maximize" | "close" | "tray") => {
    if (!desktop || closing) return;
    try {
      setError("");
      const win = getCurrentWindow();
      if (action === "minimize") await command("minimize_main_window");
      if (action === "tray") await command("tray_action", { action: "hide" });
      if (action === "maximize") {
        await win.toggleMaximize();
        setMaximized(await win.isMaximized());
      }
      if (action === "close") {
        await command("close_main_window");
      }
    } catch (e) {
      setClosing(false);
      setError(errorText(e));
    }
  };
  return (
    <div
      className={
        "desktop-frame" +
        (mac ? " frame-mac" : "") +
        (maximized ? " is-maximized" : "") +
        (fullscreen ? " is-fullscreen" : "") +
        (!focused ? " is-unfocused" : "")
      }
    >
      <header className="window-titlebar" aria-label="Управление окном">
        <div
          className="window-drag-area"
          onMouseDown={(e) => {
            if (!desktop || e.button !== 0) return;
            if (e.detail === 2) void run("maximize");
            else
              void getCurrentWindow()
                .startDragging()
                .catch((e) => setError(errorText(e)));
          }}
        >
          <BrandMark size={20} />
          <span>Babel Hack</span>
          {closing && <small role="status">Завершение сессии…</small>}
        </div>
        <div className="window-controls">
          {desktop && config?.tray_available && (
            <button
              className="window-tray"
              aria-label="Свернуть в трей"
              title="Свернуть в трей"
              disabled={closing}
              onClick={() => void run("tray")}
            >
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M3 10v3h10v-3M8 2v7m-3-3 3 3 3-3" />
              </svg>
            </button>
          )}
          <button
            className="window-minimize"
            aria-label="Свернуть окно"
            title="Свернуть"
            disabled={!desktop || closing}
            onClick={() => void run("minimize")}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path d="M3 8.5h10" />
            </svg>
          </button>
          <button
            className="window-maximize"
            aria-label={
              maximized ? "Восстановить размер окна" : "Развернуть окно"
            }
            title={maximized ? "Восстановить размер" : "Развернуть"}
            disabled={!desktop || closing}
            onClick={() => void run("maximize")}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              {maximized ? (
                <path d="M5.5 5.5v-3h8v8h-3m-8-5h8v8h-8z" />
              ) : (
                <rect x="3.5" y="3.5" width="9" height="9" />
              )}
            </svg>
          </button>
          <button
            className="window-close"
            aria-label={
              closeToTray ? "Закрыть окно в трей" : "Закрыть приложение"
            }
            title={
              closeToTray
                ? "Скрыть в трей — перевод продолжится"
                : "Выйти из приложения"
            }
            disabled={!desktop || closing}
            onClick={() => void run("close")}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path d="m3.5 3.5 9 9m-9 0 9-9" />
            </svg>
          </button>
        </div>
      </header>
      {error && (
        <div className="window-error" role="alert">
          {error}
          <button
            aria-label="Скрыть ошибку управления окном"
            onClick={() => setError("")}
          >
            ×
          </button>
        </div>
      )}
      {children}
      {desktop &&
        !maximized &&
        !fullscreen &&
        edges.map((edge) => (
          <div
            key={edge}
            className={"window-resize-edge edge-" + edge.toLowerCase()}
            aria-hidden="true"
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              e.preventDefault();
              void getCurrentWindow()
                .startResizeDragging(edge)
                .catch((e) => setError(errorText(e)));
            }}
          />
        ))}
    </div>
  );
}
