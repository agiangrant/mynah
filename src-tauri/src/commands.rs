//! Tauri commands — the IPC surface described in `src/lib/ipc.ts`.

use crate::audio::{self, AudioDevice};
use crate::dictation::DictationState;
use crate::engine::EngineStatus;
use crate::history::HistoryEntry;
use crate::meetings::MeetingState;
use crate::models::ModelInfo;
use crate::permissions::{self, PermissionKind, Permissions};
use crate::settings::Settings;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e}")
}

// ---- settings ---------------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let old = state.settings.get();
    let new = state.settings.replace(settings);

    if old.shortcuts.dictation != new.shortcuts.dictation
        || old.shortcuts.meeting_toggle != new.shortcuts.meeting_toggle
        || old.shortcuts.cancel != new.shortcuts.cancel
    {
        state.hotkeys.update(&new.shortcuts);
    }
    if old.model.selected_model_id != new.model.selected_model_id || old.model.use_gpu != new.model.use_gpu {
        state.reload_engine();
    }
    if old.dictation.keep_mic_warm_secs != new.dictation.keep_mic_warm_secs {
        state.hub.set_keep_warm(std::time::Duration::from_secs(new.dictation.keep_mic_warm_secs as u64));
    }
    if old.general.launch_at_login != new.general.launch_at_login {
        let al = app.autolaunch();
        let r = if new.general.launch_at_login { al.enable() } else { al.disable() };
        if let Err(e) = r {
            log::warn!("autostart update failed: {e}");
        }
    }
    let _ = app.emit("settings-changed", new.clone());
    Ok(new)
}

// ---- models -----------------------------------------------------------------

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<ModelInfo> {
    state.models.list()
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> CmdResult<()> {
    let state = app.state::<AppState>();
    state.models.download(app.clone(), id.clone()).await.map_err(err)?;
    // Auto-select the first usable model.
    let settings = state.settings.get();
    let selected_ok = settings
        .model
        .selected_model_id
        .as_deref()
        .and_then(|s| state.models.path_for(s))
        .is_some();
    if !selected_ok {
        state.settings.update(|s| s.model.selected_model_id = Some(id));
        let _ = app.emit("settings-changed", state.settings.get());
        state.reload_engine();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) {
    state.models.cancel_download(&id);
}

#[tauri::command]
pub fn delete_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.models.delete(&id).map_err(err)?;
    if state.engine.model_id().as_deref() == Some(id.as_str()) {
        state.settings.update(|s| s.model.selected_model_id = None);
        let _ = app.emit("settings-changed", state.settings.get());
        state.reload_engine();
    }
    let _ = app.emit("models-changed", ());
    Ok(())
}

#[tauri::command]
pub fn import_custom_model(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<ModelInfo> {
    let info = state.models.import_custom(std::path::Path::new(&path)).map_err(err)?;
    let _ = app.emit("models-changed", ());
    Ok(info)
}

#[tauri::command]
pub fn select_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.models.path_for(&id).ok_or_else(|| format!("model {id} is not installed"))?;
    state.settings.update(|s| s.model.selected_model_id = Some(id));
    let _ = app.emit("settings-changed", state.settings.get());
    state.reload_engine();
    Ok(())
}

#[tauri::command]
pub fn get_engine_status(state: State<'_, AppState>) -> EngineStatus {
    state.engine.status()
}

// ---- audio ------------------------------------------------------------------

#[tauri::command]
pub fn list_audio_devices() -> Vec<AudioDevice> {
    audio::list_devices()
}

// ---- shortcuts --------------------------------------------------------------

#[tauri::command]
pub fn start_shortcut_capture(state: State<'_, AppState>) {
    state.hotkeys.begin_capture();
}

#[tauri::command]
pub fn cancel_shortcut_capture(state: State<'_, AppState>) {
    state.hotkeys.cancel_capture();
}

// ---- dictation --------------------------------------------------------------

#[tauri::command]
pub fn start_dictation(state: State<'_, AppState>, insert: bool) {
    state.dictation.start(insert, true);
}

#[tauri::command]
pub fn stop_dictation(state: State<'_, AppState>) {
    state.dictation.stop();
}

#[tauri::command]
pub fn cancel_dictation(state: State<'_, AppState>) {
    state.dictation.cancel();
}

#[tauri::command]
pub fn get_dictation_state(state: State<'_, AppState>) -> DictationState {
    state.dictation.state()
}

// ---- history ----------------------------------------------------------------

#[tauri::command]
pub fn get_history(state: State<'_, AppState>, query: String, limit: usize, offset: usize) -> Vec<HistoryEntry> {
    state.history.query(&query, limit.clamp(1, 1000), offset)
}

#[tauri::command]
pub fn delete_history_entry(app: AppHandle, state: State<'_, AppState>, id: String) {
    state.history.delete(&id);
    let _ = app.emit("history-updated", ());
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: State<'_, AppState>) {
    state.history.clear();
    let _ = app.emit("history-updated", ());
}

// ---- meetings ---------------------------------------------------------------

#[tauri::command]
pub fn start_meeting(state: State<'_, AppState>, app: Option<String>) -> CmdResult<()> {
    if !state.engine.is_ready() {
        return Err("No model loaded".into());
    }
    let name = app.or_else(|| state.meetings.state().pending_app);
    state.meetings.start(name, false);
    Ok(())
}

#[tauri::command]
pub fn stop_meeting(state: State<'_, AppState>) {
    state.meetings.stop();
}

#[tauri::command]
pub fn get_meeting_state(state: State<'_, AppState>) -> MeetingState {
    state.meetings.state()
}

#[tauri::command]
pub fn dismiss_meeting_prompt(state: State<'_, AppState>) {
    state.meetings.dismiss_prompt();
}

// ---- permissions / system ---------------------------------------------------

#[tauri::command]
pub fn check_permissions() -> Permissions {
    permissions::check()
}

#[tauri::command]
pub fn request_permission(kind: PermissionKind) {
    permissions::request(kind);
}

#[tauri::command]
pub fn open_permission_settings(kind: PermissionKind) {
    permissions::open_settings(kind);
}

#[derive(Serialize)]
pub struct SystemInfo {
    os: String,
    arch: String,
    cpu_cores: usize,
    memory_gb: f64,
    gpu_backend: String,
    app_version: String,
    models_dir: String,
    data_dir: String,
    platform: &'static str,
}

#[tauri::command]
pub fn get_system_info(app: AppHandle, state: State<'_, AppState>) -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let settings = state.settings.get();
    SystemInfo {
        os: sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: num_cpus::get_physical(),
        memory_gb: (sys.total_memory() as f64 / 1_073_741_824.0 * 10.0).round() / 10.0,
        gpu_backend: crate::engine::gpu_backend_name(settings.model.use_gpu).to_string(),
        app_version: app.package_info().version.to_string(),
        models_dir: state.models.dir().to_string_lossy().to_string(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        platform: if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "linux"
        },
    }
}

#[tauri::command]
pub fn copy_text(text: String) -> CmdResult<()> {
    crate::inject::copy_text(&text).map_err(err)
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(&path).map_err(err)
}

#[tauri::command]
pub fn quit_app(app: AppHandle, state: State<'_, AppState>) {
    state.meetings.stop();
    state.hub.shutdown();
    app.exit(0);
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    crate::tray::show_main(&app);
}
