import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DEFAULT_CONFIG, EMPTY_STATE } from "./types";
import type { ConfigStatus, Entry, SessionRecord, SessionState } from "./types";

export const desktop = isTauri();
type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();
let previewConfig = { ...DEFAULT_CONFIG };
let previewState = { ...EMPTY_STATE };
let previewRecord: SessionRecord | null = null;
let timer: ReturnType<typeof setInterval> | undefined;
let step = 0;
const demoLines = [
  [
    "Thanks for joining today. Could you tell me about yourself?",
    "Спасибо, что присоединились. Расскажите немного о себе.",
  ],
  [
    "I build reliable products and enjoy solving difficult problems.",
    "Я создаю надёжные продукты и люблю решать сложные задачи.",
  ],
  [
    "How do you approach a project with a tight deadline?",
    "Как вы подходите к проекту с жёсткими сроками?",
  ],
  [
    "First, I identify the most important outcome and break the work into small steps.",
    "Сначала определяю главный результат и разбиваю работу на небольшие шаги.",
  ],
  [
    "Clear communication helps the team make better decisions.",
    "Понятная коммуникация помогает команде принимать лучшие решения.",
  ],
];
function emit(name: string, data: unknown) {
  handlers.get(name)?.forEach((fn) => fn(structuredClone(data)));
}
function update(patch: Partial<SessionState>) {
  previewState = {
    ...previewState,
    ...patch,
    version: previewState.version + 1,
  };
  emit("session-state", previewState);
}
export async function on<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (desktop) return listen<T>(name, (e) => handler(e.payload));
  const set = handlers.get(name) ?? new Set<Handler>();
  handlers.set(name, set);
  set.add(handler as Handler);
  return () => {
    set.delete(handler as Handler);
  };
}
export async function command<T = void>(
  name: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (desktop) return invoke<T>(name, args);
  let result: unknown;
  switch (name) {
    case "get_config":
      result = previewConfig;
      break;
    case "set_config": {
      const a = args.args as { config: ConfigStatus };
      previewConfig = { ...a.config, has_deepgram: false, has_openai: false };
      emit("config-changed", null);
      break;
    }
    case "list_output_devices":
      result = [];
      break;
    case "get_session_state":
      result = previewState;
      break;
    case "list_sessions":
      result = [];
      break;
    case "get_session":
      result = previewRecord;
      break;
    case "start_session": {
      if (!args.demo)
        throw new Error(
          "Захват системного звука доступен в настольном приложении. Здесь можно запустить демо.",
        );
      if (previewState.active) throw new Error("Сессия уже запущена");
      const now = Date.now();
      previewRecord = {
        id: String(now),
        title: "Знакомство с переводчиком",
        started_at: now,
        ended_at: null,
        source_language: "en",
        target_language: "ru",
        entries: [],
      };
      update({
        ...EMPTY_STATE,
        active: true,
        demo: true,
        status: "listening",
        session_id: String(now),
        started_at: now,
      });
      step = 0;
      timer = setInterval(() => {
        if (previewState.paused || !previewRecord) return;
        const index = Math.floor(step / 30),
          phase = step % 30;
        if (index >= demoLines.length) {
          clearInterval(timer);
          update({
            source: "",
            message:
              "Демонстрация завершена. Завершите сессию, чтобы запустить её снова.",
          });
          return;
        }
        const [source, target] = demoLines[index];
        const sourceWords = source.split(" ");
        const targetWords = target.split(" ");
        const spoken = sourceWords
          .slice(
            0,
            Math.ceil((sourceWords.length * Math.min(phase + 1, 15)) / 15),
          )
          .join(" ");
        update({
          source: spoken,
          current:
            phase >= 3 && phase < 23
              ? targetWords
                  .slice(
                    0,
                    Math.ceil(
                      (targetWords.length * Math.min(phase - 2, 18)) / 18,
                    ),
                  )
                  .join(" ")
              : "",
          translation_source: spoken,
          provisional: phase < 23,
          audio_level: 0.06 + (phase % 5) * 0.025,
          pending: phase >= 3 && phase < 23 ? 1 : 0,
        });
        if (phase === 23) {
          const entry: Entry = {
            id: index + 1,
            timestamp_ms: Date.now() - now,
            source,
            translation: target,
            error: null,
          };
          previewRecord.entries.push(entry);
          update({
            current: "",
            provisional: false,
            translation_source: "",
            lines: [...previewState.lines, target],
            cues: [
              ...previewState.cues,
              { id: previewRecord!.entries.length, text: target },
            ].slice(-8),
            pending: 0,
          });
          emit("history-changed", null);
        }
        step++;
      }, 140);
      break;
    }
    case "pause_session":
      update({ paused: true, audio_level: 0 });
      break;
    case "resume_session":
      update({ paused: false });
      break;
    case "stop_session":
      clearInterval(timer);
      if (previewRecord) previewRecord.ended_at = Date.now();
      update({
        active: false,
        paused: false,
        status: "idle",
        current: "",
        audio_level: 0,
        pending: 0,
      });
      emit("history-changed", null);
      break;
    case "get_metrics": {
      const empty = { count: 0, min: 0, avg: 0, p50: 0, p95: 0, max: 0 };
      result = {
        stt: empty,
        ttft: empty,
        total: empty,
        e2e: empty,
        coalesced: 0,
        stale: 0,
        last_stt: null,
        last_ttft: null,
        last_e2e: null,
      };
      break;
    }
    default:
      throw new Error("Эта функция доступна в настольном приложении.");
  }
  return structuredClone(result) as T;
}
export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
