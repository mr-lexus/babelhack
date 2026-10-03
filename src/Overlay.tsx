import {
  memo,
  Profiler,
  useEffect,
  useRef,
  useState,
  type PropsWithChildren,
  type RefObject,
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { command, desktop, errorText } from "./api";
import { useConfig, useSessionSelector } from "./hooks";
import { DEFAULT_CONFIG } from "./types";
import { BrandMark } from "./BrandMark";
import { Icon } from "./Icon";

export default function Overlay() {
  const { config } = useConfig();
  const cfg = config ?? DEFAULT_CONFIG;
  const body = useRef<HTMLDivElement>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!desktop || !config || config.platform?.wayland) return;
    const win = getCurrentWindow();
    let disposed = false;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const save = () => {
      clearTimeout(timeout);
      timeout = setTimeout(async () => {
        if (disposed) return;
        try {
          const [pos, size, scale] = await Promise.all([
            win.outerPosition(),
            win.innerSize(),
            win.scaleFactor(),
          ]);
          await command("save_overlay_geometry", {
            args: {
              x: pos.x / scale,
              y: pos.y / scale,
              width: size.width / scale,
              height: size.height / scale,
            },
          });
        } catch (e) {
          if (!disposed) setError(errorText(e));
        }
      }, 500);
    };
    const subs = [win.onMoved(save), win.onResized(save)];
    return () => {
      disposed = true;
      clearTimeout(timeout);
      subs.forEach((p) => void p.then((f) => f()).catch(() => {}));
    };
  }, [!!config, config?.platform?.wayland]);
  const color = cfg.overlay_bg_color;
  const rgb = [1, 3, 5]
    .map((i) => parseInt(color.slice(i, i + 2), 16) || 0)
    .join(",");
  return (
    <div
      className="overlay-shell"
      style={{
        background:
          "rgba(" +
          rgb +
          "," +
          (config?.platform?.overlay_transparency === false
            ? 1
            : cfg.overlay_opacity) +
          ")",
        color: cfg.overlay_text_color,
      }}
    >
      <header
        className="overlay-toolbar"
        onPointerDown={(e) => {
          if (
            e.button === 0 &&
            !(e.target as Element).closest("button") &&
            desktop
          )
            void getCurrentWindow()
              .startDragging()
              .catch((err) => setError(errorText(err)));
        }}
      >
        <RenderProbe name="status">
          <OverlayStatus />
        </RenderProbe>
        <button
          aria-label="Скрыть субтитры"
          onClick={() =>
            void command("toggle_overlay").catch((e) => setError(errorText(e)))
          }
        >
          <Icon name="close" size={16} />
        </button>
      </header>
      <div
        ref={body}
        className="overlay-content"
        style={
          {
            "--caption-size": cfg.overlay_font_size + "px",
            "--caption-history-line-height":
              cfg.overlay_history_line_height ?? 1.2,
            "--caption-history-spacing":
              (cfg.overlay_history_spacing ?? 6) + "px",
            "--caption-line-height": cfg.overlay_line_height ?? 1.25,
            "--caption-live-size": cfg.overlay_realtime_font_size + "px",
            "--caption-history": cfg.overlay_history_color,
          } as React.CSSProperties
        }
      >
        <RenderProbe name="history">
          <CaptionHistory limit={cfg.overlay_lines} />
        </RenderProbe>
        <RenderProbe name="live">
          <LiveCaption />
        </RenderProbe>
        {cfg.overlay_show_source && (
          <RenderProbe name="source">
            <CaptionSource />
          </RenderProbe>
        )}
        <OverlayMessage error={error} />
        <ScrollFollower body={body} />
      </div>
      <button
        className="resize-grip"
        aria-label="Изменить размер субтитров"
        onPointerDown={(e) => {
          e.preventDefault();
          if (desktop)
            void getCurrentWindow()
              .startResizeDragging("SouthEast")
              .catch((err) => setError(errorText(err)));
        }}
      >
        ◢
      </button>
    </div>
  );
}

// Profiling is opt-in in development only; no counters or global hooks in release.
function RenderProbe({ name, children }: PropsWithChildren<{ name: string }>) {
  if (
    !import.meta.env.DEV ||
    !new URLSearchParams(location.search).has("profile")
  )
    return children;
  return (
    <Profiler
      id={name}
      onRender={(id, phase) =>
        window.dispatchEvent(
          new CustomEvent("overlay-render", { detail: { id, phase } }),
        )
      }
    >
      {children}
    </Profiler>
  );
}

const OverlayStatus = memo(function OverlayStatus() {
  const active = useSessionSelector((s) => s.active);
  const paused = useSessionSelector((s) => s.paused);
  const status = useSessionSelector((s) => s.status);
  return (
    <>
      <span>
        <BrandMark size={20} /> BABEL HACK{" "}
        <i className={active && !paused ? "live-dot" : "idle-dot"} />
      </span>
      <span>
        {paused
          ? "Пауза"
          : status === "reconnecting"
            ? "Переподключение"
            : !active
              ? "Ожидание"
              : "LIVE"}
      </span>
    </>
  );
});
const Caption = memo(function Caption({
  id,
  text,
  untranslated,
}: {
  id: number;
  text: string;
  untranslated?: boolean;
}) {
  return (
    <RenderProbe name={"cue-" + id}>
      <p
        className="caption-final"
        data-cue-id={id}
        data-untranslated={!!untranslated}
      >
        {untranslated && (
          <span className="caption-fallback-label">
            Оригинал · перевод недоступен
          </span>
        )}
        {text}
      </p>
    </RenderProbe>
  );
});
const CaptionHistory = memo(function CaptionHistory({
  limit,
}: {
  limit: number;
}) {
  const cues = useSessionSelector((s) => s.cues);
  const session = useSessionSelector((s) => s.session_id);
  return (
    <div className="caption-history">
      {cues.slice(-limit).map((c) => (
        <Caption
          key={session + ":" + c.id}
          id={c.id}
          text={c.text}
          untranslated={c.untranslated}
        />
      ))}
    </div>
  );
});
const LiveCaption = memo(function LiveCaption() {
  const text = useSessionSelector((s) => s.current);
  const provisional = useSessionSelector((s) => s.provisional);
  const empty = useSessionSelector((s) => !s.cues.length);
  const active = useSessionSelector((s) => s.active);
  return (
    <div className="caption-live" data-provisional={provisional && !!text}>
      {text ? (
        <>
          <p>
            {text}
            <span className="typing-cursor" />
          </p>
          <span className="caption-label">
            {provisional
              ? "Фраза продолжается · перевод уточняется"
              : "Перевод"}
          </span>
        </>
      ) : empty ? (
        <p className="overlay-placeholder">
          {active ? "Ожидаю речь…" : "Субтитры появятся здесь"}
        </p>
      ) : null}
    </div>
  );
});
const CaptionSource = memo(function CaptionSource() {
  const source = useSessionSelector((s) => s.translation_source || s.source);
  return source ? <p className="overlay-source">{source}</p> : null;
});
const OverlayMessage = memo(function OverlayMessage({
  error,
}: {
  error: string;
}) {
  const message = useSessionSelector((s) => s.message);
  const pending = useSessionSelector((s) => s.pending >= 4);
  return error || message || pending ? (
    <small className="overlay-error">
      {error ||
        message ||
        "Перевод отстаёт от речи. Проверьте сеть или выберите более быструю модель."}
    </small>
  ) : null;
});
function ScrollFollower({ body }: { body: RefObject<HTMLDivElement> }) {
  const follow = useRef(true);
  const text = useSessionSelector((s) => s.current);
  const cues = useSessionSelector((s) => s.cues);
  useEffect(() => {
    const node = body.current;
    if (!node) return;
    const scrolled = () => {
      follow.current =
        node.scrollHeight - node.scrollTop - node.clientHeight < 48;
    };
    node.addEventListener("scroll", scrolled, { passive: true });
    return () => node.removeEventListener("scroll", scrolled);
  }, [body]);
  useEffect(() => {
    const id = requestAnimationFrame(() => {
      if (body.current && follow.current)
        body.current.scrollTop = body.current.scrollHeight;
    });
    return () => cancelAnimationFrame(id);
  }, [body, text, cues]);
  return null;
}
