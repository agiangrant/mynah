//! Shared application state and the engine slot.

use crate::audio::AudioHub;
use crate::dictation::Dictation;
use crate::engine::whisper::WhisperEngine;
use crate::engine::{gpu_backend_name, EngineState, EngineStatus, SttEngine, Transcript, TranscribeOptions};
use crate::history::HistoryStore;
use crate::hotkeys::HotkeyService;
use crate::meetings::Meetings;
use crate::models::ModelManager;
use crate::settings::SettingsStore;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};

pub struct EngineSlot {
    app: AppHandle,
    engine: Mutex<Option<Box<dyn SttEngine>>>,
    status: RwLock<EngineStatus>,
    generation: AtomicU64,
}

impl EngineSlot {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app,
            engine: Mutex::new(None),
            status: RwLock::new(EngineStatus {
                state: EngineState::Unloaded,
                model_id: None,
                message: Some("Download a model to get started".into()),
                backend: None,
            }),
            generation: AtomicU64::new(0),
        })
    }

    pub fn status(&self) -> EngineStatus {
        self.status.read().clone()
    }

    pub fn is_ready(&self) -> bool {
        self.status.read().state == EngineState::Ready
    }

    pub fn model_id(&self) -> Option<String> {
        self.status.read().model_id.clone()
    }

    fn set_status(&self, status: EngineStatus) {
        *self.status.write() = status.clone();
        let _ = self.app.emit("engine-status", status);
    }

    pub fn transcribe(&self, audio: &[f32], opts: &TranscribeOptions) -> anyhow::Result<Transcript> {
        let mut guard = self.engine.lock();
        let engine = guard.as_mut().ok_or_else(|| anyhow::anyhow!("no model loaded"))?;
        engine.transcribe(audio, opts)
    }

    pub fn unload(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        *self.engine.lock() = None;
        self.set_status(EngineStatus {
            state: EngineState::Unloaded,
            model_id: None,
            message: Some("Download a model to get started".into()),
            backend: None,
        });
    }

    /// Load a model on a background thread. Emits `engine-status` along the way.
    pub fn load(self: &Arc<Self>, model_id: String, path: PathBuf, use_gpu: bool) {
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.set_status(EngineStatus {
            state: EngineState::Loading,
            model_id: Some(model_id.clone()),
            message: Some("Loading model…".into()),
            backend: Some(gpu_backend_name(use_gpu).into()),
        });
        let this = self.clone();
        std::thread::Builder::new()
            .name("engine-load".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                match WhisperEngine::load(&model_id, &path, use_gpu) {
                    Ok(mut engine) => {
                        engine.warm_up();
                        if this.generation.load(Ordering::Relaxed) != gen {
                            return; // superseded
                        }
                        log::info!("model {model_id} ready in {:?}", started.elapsed());
                        *this.engine.lock() = Some(Box::new(engine));
                        this.set_status(EngineStatus {
                            state: EngineState::Ready,
                            model_id: Some(model_id),
                            message: None,
                            backend: Some(gpu_backend_name(use_gpu).into()),
                        });
                        this.app.state::<AppState>().refresh_tray();
                    }
                    Err(e) => {
                        log::error!("model load failed: {e:#}");
                        if this.generation.load(Ordering::Relaxed) != gen {
                            return;
                        }
                        this.set_status(EngineStatus {
                            state: EngineState::Error,
                            model_id: Some(model_id),
                            message: Some(format!("{e:#}")),
                            backend: None,
                        });
                    }
                }
            })
            .ok();
    }
}

pub struct TrayHandles {
    pub meeting_item: MenuItem<Wry>,
    pub model_item: MenuItem<Wry>,
}

pub struct AppState {
    pub settings: SettingsStore,
    pub history: HistoryStore,
    pub models: ModelManager,
    pub engine: Arc<EngineSlot>,
    pub hub: AudioHub,
    pub hotkeys: HotkeyService,
    pub dictation: Arc<Dictation>,
    pub meetings: Arc<Meetings>,
    pub data_dir: PathBuf,
    pub tray: Mutex<Option<TrayHandles>>,
}

impl AppState {
    /// (Re)load the engine according to current settings. Picks a sensible installed
    /// model when none is selected.
    pub fn reload_engine(&self) {
        let settings = self.settings.get();
        let mut selected = settings.model.selected_model_id.clone();
        if selected.as_deref().and_then(|id| self.models.path_for(id)).is_none() {
            let installed = self.models.list().into_iter().filter(|m| m.installed).collect::<Vec<_>>();
            let pick = installed
                .iter()
                .find(|m| m.recommended)
                .or_else(|| installed.iter().max_by_key(|m| (m.accuracy, m.speed)))
                .map(|m| m.id.clone());
            if pick != selected {
                selected = pick.clone();
                self.settings.update(|s| s.model.selected_model_id = pick);
                let _ = self.engine.app.emit("settings-changed", self.settings.get());
            }
        }
        match selected.and_then(|id| self.models.path_for(&id).map(|p| (id, p))) {
            Some((id, path)) => self.engine.load(id, path, settings.model.use_gpu),
            None => self.engine.unload(),
        }
    }

    pub fn refresh_tray(&self) {
        let tray = self.tray.lock();
        let Some(t) = tray.as_ref() else { return };
        let meeting = if self.meetings.is_active() {
            "Stop meeting transcription"
        } else {
            "Start meeting transcription"
        };
        let _ = t.meeting_item.set_text(meeting);
        let status = self.engine.status();
        let model = match status.state {
            EngineState::Ready => {
                let name = status
                    .model_id
                    .as_deref()
                    .map(|id| self.models.list().into_iter().find(|m| m.id == id).map(|m| m.name).unwrap_or(id.to_string()))
                    .unwrap_or_default();
                format!("Model: {name}")
            }
            EngineState::Loading => "Loading model…".into(),
            EngineState::Error => "Model failed to load".into(),
            EngineState::Unloaded => "No model loaded".into(),
        };
        let _ = t.model_item.set_text(model);
    }
}
