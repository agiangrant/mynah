//! Global keyboard listener (rdev). Tracks chords with press *and* release so
//! hold-to-talk works, and supports a "capture" mode used by the shortcut recorder.

use crate::settings::ShortcutSettings;
use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyEvent {
    DictationPressed,
    DictationReleased,
    /// A key outside a modifier-only dictation chord was pressed while the chord was held
    /// (user is doing Cmd+C, not dictating).
    DictationInterrupted,
    MeetingToggle,
    Cancel,
    Capture { keys: Vec<String>, done: bool },
    ListenerFailed(String),
}

#[derive(Debug, Clone, Default)]
struct Config {
    dictation: Vec<String>,
    meeting: Vec<String>,
    cancel: Vec<String>,
}

impl From<&ShortcutSettings> for Config {
    fn from(s: &ShortcutSettings) -> Self {
        Self {
            dictation: s.dictation.keys.clone(),
            meeting: s.meeting_toggle.as_ref().map(|s| s.keys.clone()).unwrap_or_default(),
            cancel: s.cancel.as_ref().map(|s| s.keys.clone()).unwrap_or_default(),
        }
    }
}

struct Shared {
    config: RwLock<Config>,
    pressed: Mutex<HashSet<String>>,
    capture: Mutex<Option<Vec<String>>>,
    dictation_held: Mutex<bool>,
    tx: Sender<HotkeyEvent>,
}

#[derive(Clone)]
pub struct HotkeyService {
    shared: Arc<Shared>,
}

pub fn is_modifier(key: &str) -> bool {
    matches!(
        key,
        "Alt" | "AltGr" | "ControlLeft" | "ControlRight" | "MetaLeft" | "MetaRight" | "ShiftLeft" | "ShiftRight" | "Function" | "CapsLock"
    )
}

#[cfg(not(target_os = "macos"))]
pub fn key_name(key: &rdev::Key) -> String {
    format!("{key:?}")
}

#[cfg(target_os = "macos")]
fn run_listener(shared: Arc<Shared>) {
    let on_key: Arc<dyn Fn(String, bool) + Send + Sync> = {
        let s = shared.clone();
        Arc::new(move |name, down| s.on_key(name, down))
    };
    crate::keytap_macos::run_forever(on_key, |msg| {
        let _ = shared.tx.send(HotkeyEvent::ListenerFailed(msg));
    });
}

/// Windows / Linux: rdev's listener is fine there.
#[cfg(not(target_os = "macos"))]
fn run_listener(shared: Arc<Shared>) {
    let mut reported = false;
    loop {
        let cb_shared = shared.clone();
        let result = rdev::listen(move |event| match event.event_type {
            rdev::EventType::KeyPress(k) => cb_shared.on_key(key_name(&k), true),
            rdev::EventType::KeyRelease(k) => cb_shared.on_key(key_name(&k), false),
            _ => {}
        });
        match result {
            Ok(()) => break,
            Err(e) => {
                if !reported {
                    log::error!("global key listener failed: {e:?} (retrying every 3s)");
                    let _ = shared.tx.send(HotkeyEvent::ListenerFailed(format!("{e:?}")));
                    reported = true;
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }
}

impl HotkeyService {
    pub fn start(settings: &ShortcutSettings, tx: Sender<HotkeyEvent>) -> Self {
        let shared = Arc::new(Shared {
            config: RwLock::new(Config::from(settings)),
            pressed: Mutex::new(HashSet::new()),
            capture: Mutex::new(None),
            dictation_held: Mutex::new(false),
            tx,
        });
        let listener = shared.clone();
        std::thread::Builder::new()
            .name("hotkeys".into())
            .spawn(move || run_listener(listener))
            .expect("spawn hotkey thread");
        Self { shared }
    }

    pub fn update(&self, settings: &ShortcutSettings) {
        *self.shared.config.write() = Config::from(settings);
    }

    pub fn begin_capture(&self) {
        *self.shared.capture.lock() = Some(Vec::new());
        let _ = self.shared.tx.send(HotkeyEvent::Capture { keys: vec![], done: false });
    }

    pub fn cancel_capture(&self) {
        *self.shared.capture.lock() = None;
    }
}

impl Shared {
    fn on_key(&self, key: String, down: bool) {
        {
            let mut pressed = self.pressed.lock();
            if down {
                if !pressed.insert(key.clone()) {
                    return; // key repeat
                }
            } else if !pressed.remove(&key) {
                // release of a key we never saw pressed (e.g. app started while held)
                return;
            }
        }

        // --- capture mode -------------------------------------------------
        {
            let mut capture = self.capture.lock();
            if let Some(keys) = capture.as_mut() {
                if down {
                    if !keys.contains(&key) {
                        keys.push(key.clone());
                    }
                    let _ = self.tx.send(HotkeyEvent::Capture { keys: keys.clone(), done: false });
                } else if self.pressed.lock().is_empty() && !keys.is_empty() {
                    let done = keys.clone();
                    *capture = None;
                    let _ = self.tx.send(HotkeyEvent::Capture { keys: done, done: true });
                }
                return;
            }
        }

        let config = self.config.read().clone();
        let pressed = self.pressed.lock().clone();
        let satisfied = |chord: &[String]| !chord.is_empty() && chord.iter().all(|k| pressed.contains(k));

        // --- dictation chord (press + release) ----------------------------
        let mut held = self.dictation_held.lock();
        if !*held && down && satisfied(&config.dictation) && config.dictation.contains(&key) {
            *held = true;
            let _ = self.tx.send(HotkeyEvent::DictationPressed);
            return;
        }
        if *held {
            if !down && config.dictation.contains(&key) {
                *held = false;
                let _ = self.tx.send(HotkeyEvent::DictationReleased);
                return;
            }
            if down && !config.dictation.contains(&key) {
                if config.dictation.iter().all(|k| is_modifier(k)) && !satisfied(&config.cancel) {
                    let _ = self.tx.send(HotkeyEvent::DictationInterrupted);
                }
            }
        }
        drop(held);

        if down {
            if satisfied(&config.cancel) && config.cancel.contains(&key) {
                let _ = self.tx.send(HotkeyEvent::Cancel);
            }
            if satisfied(&config.meeting) && config.meeting.contains(&key) {
                let _ = self.tx.send(HotkeyEvent::MeetingToggle);
            }
        }
    }
}
