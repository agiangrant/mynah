//! Speech-to-text engine abstraction. Whisper.cpp today; the trait keeps the
//! door open for Parakeet / Moonshine via sherpa-onnx later.

pub mod bias;
pub mod whisper;

use crate::settings::Decoding;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    /// "auto" or an ISO-639-1 code
    pub language: String,
    pub translate: bool,
    pub initial_prompt: Option<String>,
    pub decoding: Decoding,
    /// 0 = auto
    pub threads: u32,
    /// Vocabulary words boosted at decode time (see [`bias`]).
    pub bias_words: Vec<String>,
    /// Log-probability bonus for those words; 0 disables boosting.
    pub bias_strength: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub no_speech_prob: f32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: Option<String>,
}

pub trait SttEngine: Send {
    fn model_id(&self) -> &str;
    fn is_multilingual(&self) -> bool;
    /// `audio` is 16 kHz mono f32 in [-1, 1].
    fn transcribe(&mut self, audio: &[f32], opts: &TranscribeOptions) -> anyhow::Result<Transcript>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineState {
    Unloaded,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub state: EngineState,
    pub model_id: Option<String>,
    pub message: Option<String>,
    pub backend: Option<String>,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self { state: EngineState::Unloaded, model_id: None, message: None, backend: None }
    }
}

pub fn gpu_backend_name(use_gpu: bool) -> &'static str {
    if !use_gpu {
        return "cpu";
    }
    if cfg!(target_os = "macos") {
        "metal"
    } else if cfg!(feature = "cuda") {
        "cuda"
    } else if cfg!(any(feature = "vulkan", target_os = "windows")) {
        "vulkan"
    } else {
        "cpu"
    }
}

/// Pick a thread count: physical cores, capped, leaving one for the UI.
pub fn auto_threads() -> u32 {
    let physical = num_cpus::get_physical().max(1) as u32;
    physical.clamp(1, 8)
}
