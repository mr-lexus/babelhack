export type Config = {
  openai_model: string;
  deepgram_model: string;
  source_language: string;
  target_language: string;
  glossary: string;
  save_history: boolean;
  close_to_tray: boolean;
  minimize_to_tray: boolean;
  overlay_lines: number;
  overlay_opacity: number;
  overlay_font_size: number;
  overlay_history_line_height: number;
  overlay_history_spacing: number;
  history_font_size: number;
  history_line_height: number;
  history_entry_spacing: number;

  overlay_line_height: number;
  overlay_realtime_font_size: number;
  overlay_bg_color: string;
  overlay_text_color: string;
  overlay_history_color: string;
  overlay_show_source: boolean;
  audio_device: string | null;
  audio_device_name: string | null;
};
export type OutputDevice = { id: string; name: string };
export type PlatformInfo = {
  os: string;
  label: string;
  credential_store: string;
  audio_backend: string;
  audio_help: string;
  overlay_shortcut: string;
  wayland: boolean;
  overlay_transparency: boolean;
};
export const DEFAULT_PLATFORM: PlatformInfo = {
  os: "browser",
  label: "Предпросмотр",
  credential_store: "системном хранилище",
  audio_backend: "Демо",
  audio_help: "Захват системного звука доступен в настольном приложении.",
  overlay_shortcut: "Ctrl Alt T",
  wayland: false,
  overlay_transparency: true,
};
export type ConfigStatus = Config & {
  platform: PlatformInfo;
  credential_error: string | null;
  tray_available: boolean;
  has_deepgram: boolean;
  has_openai: boolean;
};
export type CaptionCue = { id: number; text: string; untranslated?: boolean };
export type SessionState = {
  version: number;
  active: boolean;
  demo: boolean;
  source_language: string;
  target_language: string;
  save_history: boolean;
  paused: boolean;
  status: string;
  message: string | null;
  session_id: string | null;
  started_at: number | null;
  source: string;
  current: string;
  translation_source: string;
  provisional: boolean;
  cues: CaptionCue[];
  lines: string[];
  pending: number;
  audio_level: number;
  dropped_chunks: number;
};
export type Entry = {
  id: number;
  timestamp_ms: number;
  source: string;
  translation: string;
  error: string | null;
};
export type SessionRecord = {
  id: string;
  title: string;
  started_at: number;
  ended_at: number | null;
  source_language: string;
  target_language: string;
  entries: Entry[];
};
export type SessionSummary = Omit<SessionRecord, "entries"> & {
  entry_count: number;
};
export type Stats = {
  count: number;
  min: number;
  avg: number;
  p50: number;
  p95: number;
  max: number;
};
export type MetricsPayload = {
  stt: Stats;
  ttft: Stats;
  total: Stats;
  e2e: Stats;
  coalesced: number;
  stale: number;
  last_stt: number | null;
  last_ttft: number | null;
  last_e2e: number | null;
};
export const LANGUAGES: Record<string, string> = {
  en: "English",
  ru: "Русский",
  uk: "Українська",
  de: "Deutsch",
  fr: "Français",
  es: "Español",
  pt: "Português",
  it: "Italiano",
  nl: "Nederlands",
  pl: "Polski",
  ro: "Română",
  ja: "日本語",
  ko: "한국어",
  zh: "中文",
};
export const DEFAULT_CONFIG: ConfigStatus = {
  has_deepgram: false,
  has_openai: false,
  openai_model: "gpt-4.1-mini",
  deepgram_model: "nova-3",
  source_language: "en",
  target_language: "ru",
  glossary: "",
  save_history: true,
  close_to_tray: true,
  minimize_to_tray: false,
  overlay_lines: 3,
  overlay_opacity: 0.88,
  overlay_font_size: 16,
  overlay_history_line_height: 1.2,
  overlay_history_spacing: 6.0,
  history_font_size: 13.0,
  history_line_height: 1.25,
  history_entry_spacing: 8.0,

  overlay_line_height: 1.25,
  overlay_realtime_font_size: 26,
  overlay_bg_color: "#111827",
  overlay_text_color: "#f8fafc",
  overlay_history_color: "#a9b7cc",
  overlay_show_source: true,
  audio_device: null,
  audio_device_name: null,
  platform: DEFAULT_PLATFORM,
  credential_error: null,
  tray_available: false,
};
export const EMPTY_STATE: SessionState = {
  version: 0,
  active: false,
  demo: false,
  source_language: "en",
  target_language: "ru",
  save_history: false,
  paused: false,
  status: "idle",
  message: null,
  session_id: null,
  started_at: null,
  source: "",
  current: "",
  translation_source: "",
  provisional: false,
  cues: [],
  lines: [],
  pending: 0,
  audio_level: 0,
  dropped_chunks: 0,
};
