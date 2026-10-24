import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioDevice,
  DictationState,
  EngineStatus,
  EventMap,
  HistoryEntry,
  MeetingState,
  ModelInfo,
  PermissionKind,
  Permissions,
  Settings,
  SystemInfo,
} from "./types";

// ---- settings --------------------------------------------------------------
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) =>
  invoke<Settings>("set_settings", { settings });

// ---- models / engine -------------------------------------------------------
export const listModels = () => invoke<ModelInfo[]>("list_models");
export const downloadModel = (id: string) => invoke<void>("download_model", { id });
export const cancelDownload = (id: string) => invoke<void>("cancel_download", { id });
export const deleteModel = (id: string) => invoke<void>("delete_model", { id });
export const importCustomModel = (path: string) =>
  invoke<ModelInfo>("import_custom_model", { path });
export const selectModel = (id: string) => invoke<void>("select_model", { id });
export const getEngineStatus = () => invoke<EngineStatus>("get_engine_status");

// ---- audio -----------------------------------------------------------------
export const listAudioDevices = () => invoke<AudioDevice[]>("list_audio_devices");

// ---- shortcuts -------------------------------------------------------------
export const startShortcutCapture = () => invoke<void>("start_shortcut_capture");
export const cancelShortcutCapture = () => invoke<void>("cancel_shortcut_capture");

// ---- dictation -------------------------------------------------------------
/** insert=false runs a preview: result is delivered via `dictation-state` with preview=true and nothing is typed. */
export const startDictation = (insert = true) => invoke<void>("start_dictation", { insert });
export const stopDictation = () => invoke<void>("stop_dictation");
export const cancelDictation = () => invoke<void>("cancel_dictation");
export const getDictationState = () => invoke<DictationState>("get_dictation_state");

// ---- history ---------------------------------------------------------------
export const getHistory = (query = "", limit = 200, offset = 0) =>
  invoke<HistoryEntry[]>("get_history", { query, limit, offset });
export const deleteHistoryEntry = (id: string) => invoke<void>("delete_history_entry", { id });
export const clearHistory = () => invoke<void>("clear_history");

// ---- meetings --------------------------------------------------------------
export const startMeeting = (app: string | null = null) => invoke<void>("start_meeting", { app });
export const stopMeeting = () => invoke<void>("stop_meeting");
export const getMeetingState = () => invoke<MeetingState>("get_meeting_state");
export const dismissMeetingPrompt = () => invoke<void>("dismiss_meeting_prompt");

// ---- permissions / system --------------------------------------------------
export const checkPermissions = () => invoke<Permissions>("check_permissions");
export const requestPermission = (kind: PermissionKind) =>
  invoke<void>("request_permission", { kind });
export const openPermissionSettings = (kind: PermissionKind) =>
  invoke<void>("open_permission_settings", { kind });
export const getSystemInfo = () => invoke<SystemInfo>("get_system_info");
export const copyText = (text: string) => invoke<void>("copy_text", { text });
export const revealPath = (path: string) => invoke<void>("reveal_path", { path });
export const quitApp = () => invoke<void>("quit_app");
export const showMainWindow = () => invoke<void>("show_main_window");

// ---- events ----------------------------------------------------------------
export function onEvent<K extends keyof EventMap>(
  name: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(name, (e) => handler(e.payload));
}
