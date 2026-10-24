// Dev-only stand-in for the Tauri bridge so the UI can be worked on in a plain
// browser: `pnpm dev` then open http://localhost:1420/?mock (or /overlay.html?mock).
import type { DictationState, EngineStatus, HistoryEntry, MeetingState, ModelInfo, Settings } from "../lib/types";

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Set<Handler>>();
let cbId = 1;

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((h) => h(payload));
}

const settings: Settings = {
  general: {
    launch_at_login: true,
    show_overlay: true,
    play_sounds: true,
    insert_method: "auto",
    trailing_space: true,
    input_device: null,
    theme: "system",
    start_minimized: false,
  },
  model: {
    selected_model_id: "large-v3-turbo",
    threads: 0,
    use_gpu: true,
    decoding: "balanced",
    language: "en",
    translate_to_english: false,
    vocabulary_boost: "standard",
  },
  shortcuts: {
    dictation: { keys: ["MetaRight"] },
    dictation_mode: "hold",
    meeting_toggle: null,
    cancel: { keys: ["Escape"] },
  },
  dictation: {
    remove_fillers: true,
    filler_words: ["uh", "um", "uhm", "umm", "hmm", "hm", "mm", "er", "erm", "ah", "eh", "mhm", "uh-huh"],
    auto_capitalize: true,
    min_speech_ms: 300,
    keep_mic_warm_secs: 30,
    hallucination_filter: true,
  },
  vocabulary: [
    { id: "1", word: "Mynah", pronunciations: ["my nah", "minah"], enabled: true },
    { id: "2", word: "Kubernetes", pronunciations: ["cooper netties"], enabled: true },
    { id: "3", word: "Tauri", pronunciations: [], enabled: false },
  ],
  meetings: {
    detection_enabled: true,
    auto_start: false,
    apps: ["zoom.us", "Microsoft Teams", "Slack", "Discord", "Webex", "FaceTime"],
    require_mic_in_use: true,
    include_system_audio: false,
    system_audio_device: null,
    save_dir: null,
    save_format: "md",
    end_after_idle_secs: 60,
  },
};

const models: ModelInfo[] = [
  { id: "large-v3-turbo", name: "Large v3 Turbo", description: "Best balance of accuracy and speed on Apple silicon.", size_bytes: 1_620_000_000, url: "x", multilingual: true, speed: 4, accuracy: 5, recommended: true, quantized: false, installed: true, path: "/m/ggml-large-v3-turbo.bin", custom: false, downloading: false },
  { id: "small.en", name: "Small (English)", description: "Quick and light. Good on laptops without a GPU.", size_bytes: 488_000_000, url: "x", multilingual: false, speed: 5, accuracy: 3, recommended: true, quantized: false, installed: false, path: null, custom: false, downloading: false },
  { id: "base.en", name: "Base (English)", description: "Tiny footprint. Fine for short commands.", size_bytes: 148_000_000, url: "x", multilingual: false, speed: 5, accuracy: 2, recommended: false, quantized: false, installed: false, path: null, custom: false, downloading: false },
  { id: "medium.en", name: "Medium (English)", description: "Noticeably better than Small at twice the latency.", size_bytes: 1_530_000_000, url: "x", multilingual: false, speed: 3, accuracy: 4, recommended: false, quantized: false, installed: false, path: null, custom: false, downloading: false },
  { id: "large-v3", name: "Large v3", description: "Highest accuracy, slowest.", size_bytes: 3_100_000_000, url: "x", multilingual: true, speed: 2, accuracy: 5, recommended: false, quantized: false, installed: false, path: null, custom: false, downloading: false },
  { id: "large-v3-turbo-q5_0", name: "Large v3 Turbo (5-bit)", description: "Same model, a third of the download, nearly the same accuracy.", size_bytes: 574_000_000, url: "x", multilingual: true, speed: 4, accuracy: 5, recommended: false, quantized: true, installed: false, path: null, custom: false, downloading: false },
];

let engine: EngineStatus = { state: "ready", model_id: "large-v3-turbo", message: null, backend: "metal" };
let dictation: DictationState = { phase: "idle", text: null, message: null, duration_ms: null, latency_ms: null, preview: false, progress: null };
const meeting: MeetingState = { active: false, app: null, started_at: null, segments: [], entry_id: null, pending_app: null };

const history: HistoryEntry[] = [
  { id: "h1", kind: "dictation", created_at: new Date(Date.now() - 120_000).toISOString(), text: "Can we move the sync to Thursday afternoon? I have a dentist appointment in the morning.", duration_ms: 4800, model_id: "large-v3-turbo", title: null, app: null, segments: null, saved_path: null },
  { id: "h2", kind: "meeting", created_at: new Date(Date.now() - 86_400_000 * 1.2).toISOString(), text: "...", duration_ms: 1_920_000, model_id: "large-v3-turbo", title: "Zoom meeting", app: "zoom.us", segments: [ { start_ms: 1000, end_ms: 4000, text: "Let's start with the roadmap.", source: "system" }, { start_ms: 5000, end_ms: 9000, text: "Sure. The model manager shipped yesterday.", source: "mic" } ], saved_path: "/Users/me/Documents/Mynah/2026-10-22 Zoom.md" },
];

const progress: Record<string, number> = {};

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 30));
  switch (cmd) {
    case "plugin:event|listen": {
      const ev = args.event as string;
      const h = (window as unknown as Record<string, Handler>)[`_${args.handler as number}`];
      if (!listeners.has(ev)) listeners.set(ev, new Set());
      listeners.get(ev)!.add((p) => h({ event: ev, id: 0, payload: p }));
      return 0;
    }
    case "plugin:event|unlisten":
      return;
    case "get_settings":
      return settings;
    case "set_settings":
      Object.assign(settings, args.settings as Settings);
      return settings;
    case "list_models":
      return models;
    case "get_engine_status":
      return engine;
    case "select_model": {
      engine = { state: "loading", model_id: args.id as string, message: null, backend: null };
      emit("engine-status", engine);
      setTimeout(() => {
        engine = { state: "ready", model_id: args.id as string, message: null, backend: "metal" };
        settings.model.selected_model_id = args.id as string;
        emit("engine-status", engine);
        emit("settings-changed", settings);
      }, 1200);
      return;
    }
    case "download_model": {
      const id = args.id as string;
      const m = models.find((x) => x.id === id)!;
      m.downloading = true;
      progress[id] = 0;
      const t = setInterval(() => {
        progress[id] += m.size_bytes / 40;
        if (progress[id] >= m.size_bytes) {
          clearInterval(t);
          m.downloading = false;
          m.installed = true;
          emit("model-download-progress", { id, downloaded: m.size_bytes, total: m.size_bytes, done: true, error: null });
          emit("models-changed", null);
        } else emit("model-download-progress", { id, downloaded: progress[id], total: m.size_bytes, done: false, error: null });
      }, 150);
      return;
    }
    case "cancel_download":
    case "delete_model":
      return;
    case "list_audio_devices":
      return [
        { name: "MacBook Pro Microphone", is_default: true, kind: "input" },
        { name: "AirPods Pro", is_default: false, kind: "input" },
        { name: "BlackHole 2ch", is_default: false, kind: "input" },
        { name: "MacBook Pro Speakers", is_default: true, kind: "output" },
      ];
    case "start_shortcut_capture":
      setTimeout(() => emit("shortcut-capture", { keys: ["ControlLeft"], done: false }), 600);
      setTimeout(() => emit("shortcut-capture", { keys: ["ControlLeft", "Space"], done: false }), 1000);
      setTimeout(() => emit("shortcut-capture", { keys: ["ControlLeft", "Space"], done: true }), 1600);
      return;
    case "cancel_shortcut_capture":
      return;
    case "start_dictation": {
      dictation = { ...dictation, phase: "listening", preview: !(args.insert as boolean) };
      emit("dictation-state", dictation);
      const t = setInterval(() => emit("audio-level", { rms: 0.05 + Math.random() * 0.2, peak: 0.5 }), 50);
      (window as unknown as Record<string, unknown>).__lvl = t;
      return;
    }
    case "stop_dictation": {
      clearInterval((window as unknown as Record<string, number>).__lvl);
      const preview = dictation.preview;
      dictation = { ...dictation, phase: "transcribing" };
      emit("dictation-state", dictation);
      setTimeout(() => {
        dictation = { phase: "inserted", text: "This is what the model heard, with the fillers already removed.", message: null, duration_ms: 3200, latency_ms: 410, preview, progress: null };
        emit("dictation-state", dictation);
        setTimeout(() => {
          dictation = { ...dictation, phase: "idle" };
          emit("dictation-state", dictation);
        }, 1500);
      }, 700);
      return;
    }
    case "cancel_dictation":
      return;
    case "get_dictation_state":
      return dictation;
    case "get_history":
      return history.filter((h) => !args.query || h.text.toLowerCase().includes(String(args.query).toLowerCase()));
    case "delete_history_entry":
    case "clear_history":
      return;
    case "start_meeting":
      Object.assign(meeting, { active: true, app: (args.app as string) ?? "Manual", started_at: new Date().toISOString(), segments: [], pending_app: null });
      emit("meeting-state", meeting);
      setTimeout(() => emit("meeting-segment", { start_ms: 2000, end_ms: 5000, text: "Morning everyone, let's get started.", source: "system" }), 2000);
      return;
    case "stop_meeting":
      Object.assign(meeting, { active: false, app: null, started_at: null });
      emit("meeting-state", meeting);
      return;
    case "get_meeting_state":
      return meeting;
    case "dismiss_meeting_prompt":
      meeting.pending_app = null;
      emit("meeting-state", meeting);
      return;
    case "check_permissions":
      return { accessibility: true, input_monitoring: false, microphone: true };
    case "request_permission":
    case "open_permission_settings":
    case "copy_text":
    case "reveal_path":
    case "quit_app":
    case "show_main_window":
      return;
    case "get_system_info":
      return { os: "macOS 26.0", arch: "aarch64", cpu_cores: 12, memory_gb: 36, gpu_backend: "metal", app_version: "0.1.0", models_dir: "/Users/me/Library/Application Support/app.mynah.desktop/models", data_dir: "/Users/me/Library/Application Support/app.mynah.desktop", platform: "macos" };
    case "plugin:autostart|enable":
    case "plugin:autostart|disable":
      return;
    default:
      throw new Error(`mock: unknown command ${cmd}`);
  }
}

(window as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener() {},
};

(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback(cb: Handler) {
    const id = cbId++;
    (window as unknown as Record<string, Handler>)[`_${id}`] = cb;
    return id;
  },
  unregisterCallback() {},
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
};

// expose a few knobs for manual testing in the console
(window as unknown as Record<string, unknown>).mynahMock = {
  promptMeeting(app = "zoom.us") {
    meeting.pending_app = app;
    emit("meeting-state", meeting);
  },
  dictation(phase: DictationState["phase"], text = "Hello from the overlay") {
    dictation = { ...dictation, phase, text };
    emit("dictation-state", dictation);
  },
  level(rms: number) {
    emit("audio-level", { rms, peak: rms * 2 });
  },
};
