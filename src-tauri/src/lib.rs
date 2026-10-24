pub mod audio;
mod commands;
mod dictation;
pub mod engine;
mod history;
mod hotkeys;
pub mod inject;
#[cfg(target_os = "macos")]
mod keytap_macos;
mod logging;
mod meetings;
mod models;
mod overlay;
mod permissions;
pub mod postprocess;
pub mod settings;
mod sounds;
mod state;
mod tray;

use audio::AudioHub;
use dictation::Dictation;
use history::HistoryStore;
use hotkeys::{HotkeyEvent, HotkeyService};
use meetings::Meetings;
use models::ModelManager;
use parking_lot::Mutex;
use settings::SettingsStore;
use state::{AppState, EngineSlot};
use tauri::{Emitter, Manager, WindowEvent};

pub fn run() {
    logging::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::import_custom_model,
            commands::select_model,
            commands::get_engine_status,
            commands::list_audio_devices,
            commands::start_shortcut_capture,
            commands::cancel_shortcut_capture,
            commands::start_dictation,
            commands::stop_dictation,
            commands::cancel_dictation,
            commands::get_dictation_state,
            commands::get_history,
            commands::delete_history_entry,
            commands::clear_history,
            commands::start_meeting,
            commands::stop_meeting,
            commands::get_meeting_state,
            commands::dismiss_meeting_prompt,
            commands::check_permissions,
            commands::request_permission,
            commands::open_permission_settings,
            commands::get_system_info,
            commands::copy_text,
            commands::reveal_path,
            commands::quit_app,
            commands::show_main_window,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            logging::attach_file(&data_dir);
            log::info!("Mynah {} starting; data dir: {}", env!("CARGO_PKG_VERSION"), data_dir.display());
            {
                let p = permissions::check();
                log::info!(
                    "permissions: accessibility={:?} input_monitoring={:?} microphone={:?}",
                    p.accessibility, p.input_monitoring, p.microphone
                );
            }

            let settings = SettingsStore::load(data_dir.join("settings.json"));
            let history = HistoryStore::load(data_dir.join("history.jsonl"));
            let models = ModelManager::new(data_dir.join("models"));
            let hub = AudioHub::new();
            let engine = EngineSlot::new(handle.clone());
            let dictation = Dictation::new(handle.clone(), hub.clone());
            let meetings = Meetings::new(handle.clone(), hub.clone());

            let (hk_tx, hk_rx) = crossbeam_channel::unbounded::<HotkeyEvent>();
            let hotkeys = HotkeyService::start(&settings.get().shortcuts, hk_tx);

            let current = settings.get();
            hub.set_keep_warm(std::time::Duration::from_secs(current.dictation.keep_mic_warm_secs as u64));

            app.manage(AppState {
                settings,
                history,
                models,
                engine,
                hub,
                hotkeys,
                dictation: dictation.clone(),
                meetings: meetings.clone(),
                data_dir,
                tray: Mutex::new(None),
            });

            tray::build(&handle)?;
            handle.state::<AppState>().reload_engine();

            // Hotkey event pump
            {
                let app = handle.clone();
                std::thread::Builder::new()
                    .name("hotkey-events".into())
                    .spawn(move || {
                        for ev in hk_rx {
                            let state = app.state::<AppState>();
                            match ev {
                                HotkeyEvent::DictationPressed => state.dictation.on_hotkey_pressed(),
                                HotkeyEvent::DictationReleased => state.dictation.on_hotkey_released(),
                                HotkeyEvent::DictationInterrupted => state.dictation.cancel(),
                                HotkeyEvent::Cancel => state.dictation.cancel(),
                                HotkeyEvent::MeetingToggle => state.meetings.toggle(),
                                HotkeyEvent::Capture { keys, done } => {
                                    let _ = app.emit("shortcut-capture", serde_json::json!({ "keys": keys, "done": done }));
                                }
                                HotkeyEvent::ListenerFailed(msg) => {
                                    log::error!("hotkey listener unavailable: {msg}");
                                }
                            }
                        }
                    })?;
            }

            // Meeting detector
            {
                let meetings = meetings.clone();
                std::thread::Builder::new()
                    .name("meeting-detector".into())
                    .spawn(move || loop {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        meetings.tick();
                    })?;
            }

            // Start hidden when requested (launch at login / --minimized)
            let minimized = current.general.start_minimized || std::env::args().any(|a| a == "--minimized");
            if minimized {
                if let Some(w) = app.get_webview_window(tray::MAIN) {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == tray::MAIN {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    // Keep running in the tray.
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Mynah")
        .run(|app, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            tauri::RunEvent::Exit => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.meetings.stop();
                    state.hub.shutdown();
                    state.engine.unload();
                }
                log::info!("exiting");
                log::logger().flush();
                // Leave without running C/C++ static destructors: ggml/Metal teardown at
                // exit has been seen to abort() and leave a crash report behind.
                unsafe { _exit(0) }
            }
            _ => {}
        });
}

extern "C" {
    fn _exit(code: i32) -> !;
}
