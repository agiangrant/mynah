// ---------------------------------------------------------------------------
// Mynah IPC contract — mirrors src-tauri/src/settings.rs and friends exactly.
// Field names are snake_case because they are serialized by serde on the Rust side.
// ---------------------------------------------------------------------------

export type InsertMethod = "auto" | "type" | "paste";
export type Theme = "system" | "light" | "dark";
export type DictationMode = "hold" | "toggle" | "auto";
export type Decoding = "fast" | "balanced" | "accurate";
/** decode-time boost for vocabulary words */
export type VocabBoost = "off" | "gentle" | "standard" | "strong";
export type SaveFormat = "md" | "txt";

/** Canonical key names come from rdev's `Key` enum Debug form, e.g. "MetaRight", "ControlLeft", "Space", "KeyD", "Function", "F5". */
export interface Shortcut {
  keys: string[];
}

export interface GeneralSettings {
  launch_at_login: boolean;
  show_overlay: boolean;
  play_sounds: boolean;
  insert_method: InsertMethod;
  trailing_space: boolean;
  /** cpal device name; null = system default */
  input_device: string | null;
  theme: Theme;
  /** keep the main window hidden on launch (start in tray) */
  start_minimized: boolean;
}

export interface ModelSettings {
  selected_model_id: string | null;
  /** 0 = auto */
  threads: number;
  use_gpu: boolean;
  decoding: Decoding;
  /** "auto" or ISO-639-1 code such as "en" */
  language: string;
  translate_to_english: boolean;
  vocabulary_boost: VocabBoost;
}

export interface ShortcutSettings {
  dictation: Shortcut;
  dictation_mode: DictationMode;
  meeting_toggle: Shortcut | null;
  /** key that cancels an in-progress dictation; default Escape */
  cancel: Shortcut | null;
}

export interface DictationSettings {
  remove_fillers: boolean;
  filler_words: string[];
  auto_capitalize: boolean;
  /** recordings shorter than this are ignored */
  min_speech_ms: number;
  /** keep the mic stream open this long after use for instant re-trigger; 0 = close immediately */
  keep_mic_warm_secs: number;
  /** drop well-known Whisper hallucinations on silent audio */
  hallucination_filter: boolean;
}

export interface VocabEntry {
  id: string;
  /** the correct spelling to insert */
  word: string;
  /** how the model tends to hear it, e.g. ["my nah", "minah"] */
  pronunciations: string[];
  enabled: boolean;
}

export interface MeetingSettings {
  detection_enabled: boolean;
  /** true = start transcribing automatically; false = ask first */
  auto_start: boolean;
  /** process-name substrings that count as a meeting app */
  apps: string[];
  require_mic_in_use: boolean;
  include_system_audio: boolean;
  /** device used to capture the other participants (loopback / BlackHole / aggregate) */
  system_audio_device: string | null;
  /** null = <app data>/meetings */
  save_dir: string | null;
  save_format: SaveFormat;
  /** stop automatically after this much silence once the meeting app is gone */
  end_after_idle_secs: number;
}

export interface Settings {
  general: GeneralSettings;
  model: ModelSettings;
  shortcuts: ShortcutSettings;
  dictation: DictationSettings;
  vocabulary: VocabEntry[];
  meetings: MeetingSettings;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  size_bytes: number;
  url: string | null;
  multilingual: boolean;
  /** 1 (slow) … 5 (fastest) */
  speed: number;
  /** 1 … 5 (most accurate) */
  accuracy: number;
  recommended: boolean;
  quantized: boolean;
  installed: boolean;
  path: string | null;
  custom: boolean;
  downloading: boolean;
}

export type EngineState = "unloaded" | "loading" | "ready" | "error";

export interface EngineStatus {
  state: EngineState;
  model_id: string | null;
  message: string | null;
  /** "metal" | "cuda" | "vulkan" | "cpu" */
  backend: string | null;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number | null;
  done: boolean;
  error: string | null;
}

export type DictationPhase =
  | "idle"
  | "listening"
  | "transcribing"
  | "inserted"
  | "cancelled"
  | "error";

export interface DictationState {
  phase: DictationPhase;
  /** final text once inserted (or when insert=false, the result for the UI) */
  text: string | null;
  message: string | null;
  /** length of the recording */
  duration_ms: number | null;
  /** time from key release to the text being ready */
  latency_ms: number | null;
  /** true when dictation was started from the UI with insert=false */
  preview: boolean;
  /** percent of the recording already transcribed while phase is "transcribing" */
  progress: number | null;
}

export interface AudioLevel {
  rms: number;
  peak: number;
}

export interface ShortcutCapture {
  keys: string[];
  done: boolean;
}

export type HistoryKind = "dictation" | "meeting";

export interface MeetingSegment {
  start_ms: number;
  end_ms: number;
  text: string;
  /** "mic" = you, "system" = other participants */
  source: "mic" | "system";
}

export interface HistoryEntry {
  id: string;
  kind: HistoryKind;
  /** RFC 3339 */
  created_at: string;
  text: string;
  duration_ms: number;
  model_id: string | null;
  title: string | null;
  app: string | null;
  segments: MeetingSegment[] | null;
  saved_path: string | null;
}

export interface MeetingState {
  active: boolean;
  app: string | null;
  started_at: string | null;
  segments: MeetingSegment[];
  entry_id: string | null;
  /** an app was detected and we are waiting for the user to accept/dismiss */
  pending_app: string | null;
}

export interface MeetingDetected {
  app: string;
  process: string;
}

export interface AudioDevice {
  name: string;
  is_default: boolean;
  kind: "input" | "output";
}

export type PermissionKind = "accessibility" | "input_monitoring" | "microphone";

/** null = not applicable on this platform */
export interface Permissions {
  accessibility: boolean | null;
  input_monitoring: boolean | null;
  microphone: boolean | null;
}

export interface SystemInfo {
  os: string;
  arch: string;
  cpu_cores: number;
  memory_gb: number;
  gpu_backend: string;
  app_version: string;
  models_dir: string;
  data_dir: string;
  /** platform-specific display hints for key names */
  platform: "macos" | "windows" | "linux";
}

// ---------------------------------------------------------------------------
// Events emitted by the backend (listen with `onEvent` from ipc.ts)
// ---------------------------------------------------------------------------
export interface EventMap {
  "engine-status": EngineStatus;
  "model-download-progress": DownloadProgress;
  "dictation-state": DictationState;
  "audio-level": AudioLevel;
  "shortcut-capture": ShortcutCapture;
  "meeting-detected": MeetingDetected;
  "meeting-state": MeetingState;
  "meeting-segment": MeetingSegment;
  "settings-changed": Settings;
  "history-updated": null;
  /** tray asked the main window to open a page */
  navigate: { page: string };
  "models-changed": null;
}
