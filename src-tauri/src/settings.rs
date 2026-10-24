//! Persistent user settings. Mirrors `src/lib/types.ts` exactly.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InsertMethod {
    #[default]
    Auto,
    Type,
    Paste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DictationMode {
    #[default]
    Hold,
    Toggle,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Decoding {
    Fast,
    #[default]
    Balanced,
    Accurate,
}

/// Decode-time boost for vocabulary words (shallow fusion in whisper.cpp's logits callback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VocabBoost {
    Off,
    Gentle,
    #[default]
    Standard,
    Strong,
}

impl VocabBoost {
    /// Log-probability bonus added to tokens that continue a vocabulary word.
    /// A token only wins when it was already within exp(-bonus) of the top choice.
    pub fn strength(self) -> f32 {
        match self {
            VocabBoost::Off => 0.0,
            VocabBoost::Gentle => 2.0,
            VocabBoost::Standard => 3.5,
            VocabBoost::Strong => 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SaveFormat {
    #[default]
    Md,
    Txt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Shortcut {
    pub keys: Vec<String>,
}

impl Shortcut {
    pub fn new(keys: &[&str]) -> Self {
        Self { keys: keys.iter().map(|k| k.to_string()).collect() }
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub show_overlay: bool,
    pub play_sounds: bool,
    pub insert_method: InsertMethod,
    pub trailing_space: bool,
    pub input_device: Option<String>,
    pub theme: Theme,
    pub start_minimized: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            show_overlay: true,
            play_sounds: true,
            insert_method: InsertMethod::Auto,
            trailing_space: true,
            input_device: None,
            theme: Theme::System,
            start_minimized: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSettings {
    pub selected_model_id: Option<String>,
    pub threads: u32,
    pub use_gpu: bool,
    pub decoding: Decoding,
    pub language: String,
    pub translate_to_english: bool,
    pub vocabulary_boost: VocabBoost,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            selected_model_id: None,
            threads: 0,
            use_gpu: true,
            decoding: Decoding::Balanced,
            language: "auto".into(),
            translate_to_english: false,
            vocabulary_boost: VocabBoost::Standard,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutSettings {
    pub dictation: Shortcut,
    pub dictation_mode: DictationMode,
    pub meeting_toggle: Option<Shortcut>,
    pub cancel: Option<Shortcut>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            dictation: Shortcut::new(&[default_dictation_key()]),
            dictation_mode: DictationMode::Hold,
            meeting_toggle: Some(Shortcut::new(&["ControlLeft", "ShiftLeft", "KeyM"])),
            cancel: Some(Shortcut::new(&["Escape"])),
        }
    }
}

fn default_dictation_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "MetaRight"
    } else {
        "ControlRight"
    }
}

pub const DEFAULT_FILLERS: &[&str] = &[
    "uh", "um", "uhm", "umm", "hmm", "hm", "mm", "er", "erm", "ah", "eh", "mhm", "uh-huh",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationSettings {
    pub remove_fillers: bool,
    pub filler_words: Vec<String>,
    pub auto_capitalize: bool,
    pub min_speech_ms: u32,
    pub keep_mic_warm_secs: u32,
    pub hallucination_filter: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            remove_fillers: true,
            filler_words: DEFAULT_FILLERS.iter().map(|s| s.to_string()).collect(),
            auto_capitalize: true,
            min_speech_ms: 300,
            keep_mic_warm_secs: 30,
            hallucination_filter: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VocabEntry {
    pub id: String,
    pub word: String,
    pub pronunciations: Vec<String>,
    pub enabled: bool,
}

impl Default for VocabEntry {
    fn default() -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), word: String::new(), pronunciations: vec![], enabled: true }
    }
}

pub const DEFAULT_MEETING_APPS: &[&str] = &[
    "zoom.us",
    "CptHost",
    "Zoom",
    "MSTeams",
    "Microsoft Teams",
    "Teams",
    "ms-teams",
    "Webex",
    "FaceTime",
    "Slack",
    "Discord",
    "Skype",
    "GoToMeeting",
    "Whereby",
    "Around",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MeetingSettings {
    pub detection_enabled: bool,
    pub auto_start: bool,
    pub apps: Vec<String>,
    pub require_mic_in_use: bool,
    pub include_system_audio: bool,
    pub system_audio_device: Option<String>,
    pub save_dir: Option<String>,
    pub save_format: SaveFormat,
    pub end_after_idle_secs: u32,
}

impl Default for MeetingSettings {
    fn default() -> Self {
        Self {
            detection_enabled: true,
            auto_start: false,
            apps: DEFAULT_MEETING_APPS.iter().map(|s| s.to_string()).collect(),
            require_mic_in_use: true,
            include_system_audio: false,
            system_audio_device: None,
            save_dir: None,
            save_format: SaveFormat::Md,
            end_after_idle_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub general: GeneralSettings,
    pub model: ModelSettings,
    pub shortcuts: ShortcutSettings,
    pub dictation: DictationSettings,
    pub vocabulary: Vec<VocabEntry>,
    pub meetings: MeetingSettings,
}

impl Settings {
    /// Clamp / clean values coming from the UI.
    pub fn normalize(&mut self) {
        self.dictation.min_speech_ms = self.dictation.min_speech_ms.clamp(50, 5000);
        self.dictation.keep_mic_warm_secs = self.dictation.keep_mic_warm_secs.min(3600);
        self.dictation.filler_words = self
            .dictation
            .filler_words
            .iter()
            .map(|w| w.trim().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        self.dictation.filler_words.dedup();
        self.model.threads = self.model.threads.min(128);
        if self.model.language.trim().is_empty() {
            self.model.language = "auto".into();
        }
        for v in &mut self.vocabulary {
            v.word = v.word.trim().to_string();
            v.pronunciations = v
                .pronunciations
                .iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            if v.id.is_empty() {
                v.id = uuid::Uuid::new_v4().to_string();
            }
        }
        self.vocabulary.retain(|v| !v.word.is_empty());
        self.meetings.apps = self
            .meetings
            .apps
            .iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        self.meetings.end_after_idle_secs = self.meetings.end_after_idle_secs.clamp(5, 3600);
        if let Some(s) = &self.shortcuts.cancel {
            if s.is_empty() {
                self.shortcuts.cancel = None;
            }
        }
        if let Some(s) = &self.shortcuts.meeting_toggle {
            if s.is_empty() {
                self.shortcuts.meeting_toggle = None;
            }
        }
        if self.shortcuts.dictation.is_empty() {
            self.shortcuts.dictation = ShortcutSettings::default().dictation;
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let settings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .map(|mut s| {
                s.normalize();
                s
            })
            .unwrap_or_default();
        Self { path, inner: RwLock::new(settings) }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().clone()
    }

    pub fn update<F: FnOnce(&mut Settings)>(&self, f: F) -> Settings {
        let snapshot = {
            let mut guard = self.inner.write();
            f(&mut guard);
            guard.normalize();
            guard.clone()
        };
        if let Err(e) = self.persist(&snapshot) {
            log::error!("failed to persist settings: {e:#}");
        }
        snapshot
    }

    pub fn replace(&self, mut settings: Settings) -> Settings {
        settings.normalize();
        *self.inner.write() = settings.clone();
        if let Err(e) = self.persist(&settings) {
            log::error!("failed to persist settings: {e:#}");
        }
        settings
    }

    fn persist(&self, settings: &Settings) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}
