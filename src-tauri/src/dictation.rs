//! Push-to-talk / toggle dictation state machine.
//!
//! Audio is segmented while it is still being recorded and transcribed chunk by
//! chunk on a worker thread. A long dictation therefore finishes moments after the
//! key is released instead of minutes later, and memory stays flat however long
//! the recording runs. Short dictations are never split: the segmenter only cuts
//! once a recording passes 20 s, and then only inside a pause.

use crate::audio::{self, AudioHub, Subscription, TARGET_RATE};
use crate::engine::whisper::vocabulary_prompt;
use crate::engine::TranscribeOptions;
use crate::history::HistoryEntry;
use crate::postprocess::PostProcessor;
use crate::settings::{DictationMode, Settings};
use crate::state::AppState;
use crate::{overlay, sounds};
use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Listening,
    Transcribing,
    Inserted,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictationState {
    pub phase: Phase,
    pub text: Option<String>,
    pub message: Option<String>,
    pub duration_ms: Option<u64>,
    pub latency_ms: Option<u64>,
    pub preview: bool,
    /// percent of the recording already transcribed while `Transcribing`
    pub progress: Option<u8>,
}

impl Default for DictationState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            text: None,
            message: None,
            duration_ms: None,
            latency_ms: None,
            preview: false,
            progress: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioLevel {
    pub rms: f32,
    pub peak: f32,
}

/// Hard stop for a forgotten toggle session.
const MAX_RECORDING: Duration = Duration::from_secs(3 * 3600);
const TAP_THRESHOLD: Duration = Duration::from_millis(350);

const FATE_PENDING: u8 = 0;
const FATE_FINISH: u8 = 1;
const FATE_DISCARD: u8 = 2;

/// Filled by `stop()` for the worker to act on once the last chunk is transcribed.
struct FinishInfo {
    duration: Duration,
    stopped_at: Instant,
}

/// State shared by the collector, the transcription worker and `stop`/`cancel`.
struct Shared {
    fate: AtomicU8,
    /// samples handed to the worker so far / samples it has finished with
    samples_sent: AtomicU64,
    samples_done: AtomicU64,
    finish: Mutex<Option<FinishInfo>>,
}

struct Session {
    _sub: Subscription,
    started: Instant,
    insert: bool,
    /// true when started by a tap in auto mode (stays on until next press)
    toggled: bool,
    shared: Arc<Shared>,
}

pub struct Dictation {
    app: AppHandle,
    hub: AudioHub,
    session: Mutex<Option<Session>>,
    state: Mutex<DictationState>,
    next_id: AtomicU64,
    /// id of the most recently started session; used to decide whether a finished
    /// transcription may update the UI state
    latest_session: AtomicU64,
    press_started: Mutex<Option<Instant>>,
    hide_generation: AtomicU64,
}

impl Dictation {
    pub fn new(app: AppHandle, hub: AudioHub) -> Arc<Self> {
        Arc::new(Self {
            app,
            hub,
            session: Mutex::new(None),
            state: Mutex::new(DictationState::default()),
            next_id: AtomicU64::new(1),
            latest_session: AtomicU64::new(0),
            press_started: Mutex::new(None),
            hide_generation: AtomicU64::new(0),
        })
    }

    pub fn state(&self) -> DictationState {
        self.state.lock().clone()
    }

    pub fn is_listening(&self) -> bool {
        self.session.lock().is_some()
    }

    fn set_state(&self, st: DictationState) {
        *self.state.lock() = st.clone();
        let _ = self.app.emit("dictation-state", st);
    }

    fn settings(&self) -> Settings {
        self.app.state::<AppState>().settings.get()
    }

    // ---- hotkey driven -----------------------------------------------------

    pub fn on_hotkey_pressed(self: &Arc<Self>) {
        let mode = self.settings().shortcuts.dictation_mode;
        *self.press_started.lock() = Some(Instant::now());
        let listening = self.is_listening();
        match mode {
            DictationMode::Hold => {
                if !listening {
                    self.start(true, false);
                }
            }
            DictationMode::Toggle => {
                if listening {
                    self.stop();
                } else {
                    self.start(true, true);
                }
            }
            DictationMode::Auto => {
                if listening {
                    // a press while a toggled session is running stops it
                    let toggled = self.session.lock().as_ref().map(|s| s.toggled).unwrap_or(false);
                    if toggled {
                        self.stop();
                    }
                } else {
                    self.start(true, false);
                }
            }
        }
    }

    pub fn on_hotkey_released(self: &Arc<Self>) {
        let mode = self.settings().shortcuts.dictation_mode;
        let held_for = self.press_started.lock().take().map(|t| t.elapsed()).unwrap_or_default();
        match mode {
            DictationMode::Hold => self.stop(),
            DictationMode::Toggle => {}
            DictationMode::Auto => {
                let mut session = self.session.lock();
                if let Some(s) = session.as_mut() {
                    if held_for < TAP_THRESHOLD && !s.toggled {
                        s.toggled = true; // tap → stay on
                        return;
                    }
                    if !s.toggled {
                        drop(session);
                        self.stop();
                    }
                }
            }
        }
    }

    // ---- public control ----------------------------------------------------

    pub fn start(self: &Arc<Self>, insert: bool, toggled: bool) {
        let settings = self.settings();
        let engine_ready = self.app.state::<AppState>().engine.is_ready();
        if !engine_ready {
            self.flash_error("No model loaded. Download one in Models.");
            return;
        }
        let mut session = self.session.lock();
        if session.is_some() {
            return;
        }
        self.hub.set_keep_warm(Duration::from_secs(settings.dictation.keep_mic_warm_secs as u64));
        let sub = match self.hub.subscribe(settings.general.input_device.as_deref()) {
            Ok(s) => s,
            Err(e) => {
                drop(session);
                self.flash_error(&format!("Microphone unavailable: {e:#}"));
                return;
            }
        };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.latest_session.store(id, Ordering::Relaxed);
        let shared = Arc::new(Shared {
            fate: AtomicU8::new(FATE_PENDING),
            samples_sent: AtomicU64::new(0),
            samples_done: AtomicU64::new(0),
            finish: Mutex::new(None),
        });
        let (chunk_tx, chunk_rx) = crossbeam_channel::unbounded::<Vec<f32>>();

        // collector thread: segments frames into chunks for the worker and reports levels
        let rx = sub.rx.clone();
        let app = self.app.clone();
        let this = Arc::downgrade(self);
        let shared2 = shared.clone();
        std::thread::Builder::new()
            .name("dictation-collect".into())
            .spawn(move || {
                let mut segmenter = Segmenter::new();
                let mut out: Vec<Vec<f32>> = Vec::new();
                let mut last_level = Instant::now();
                let started = Instant::now();
                let send = |chunks: &mut Vec<Vec<f32>>| {
                    for chunk in chunks.drain(..) {
                        shared2.samples_sent.fetch_add(chunk.len() as u64, Ordering::AcqRel);
                        let _ = chunk_tx.send(chunk);
                    }
                };
                while let Ok(frame) = rx.recv() {
                    let (rms, peak) = audio::level(&frame);
                    segmenter.push(&frame, &mut out);
                    send(&mut out);
                    if last_level.elapsed() > Duration::from_millis(33) {
                        last_level = Instant::now();
                        let _ = app.emit("audio-level", AudioLevel { rms, peak });
                    }
                    if started.elapsed() > MAX_RECORDING {
                        if let Some(d) = this.upgrade() {
                            d.stop();
                        }
                        break;
                    }
                }
                segmenter.flush(&mut out);
                send(&mut out);
                // chunk_tx drops here, which ends the worker's loop once it has caught up
            })
            .ok();

        // worker thread: transcribes chunks as they arrive, then finishes the session
        let this = self.clone();
        let shared3 = shared.clone();
        let worker_settings = settings.clone();
        std::thread::Builder::new()
            .name("dictation-transcribe".into())
            .spawn(move || this.run_worker(id, chunk_rx, shared3, insert, worker_settings))
            .ok();

        *session = Some(Session { _sub: sub, started: Instant::now(), insert, toggled, shared });
        drop(session);

        self.hide_generation.fetch_add(1, Ordering::Relaxed);
        self.set_state(DictationState { phase: Phase::Listening, preview: !insert, ..Default::default() });
        if settings.general.play_sounds {
            sounds::play(sounds::Cue::Start);
        }
        if settings.general.show_overlay {
            overlay::show(&self.app);
        }
    }

    pub fn stop(self: &Arc<Self>) {
        let Some(session) = self.session.lock().take() else { return };
        let settings = self.settings();
        let duration = session.started.elapsed();
        let insert = session.insert;

        if settings.general.play_sounds {
            sounds::play(sounds::Cue::Stop);
        }
        if duration < Duration::from_millis(settings.dictation.min_speech_ms as u64) {
            session.shared.fate.store(FATE_DISCARD, Ordering::Release);
            drop(session);
            self.set_state(DictationState {
                phase: Phase::Cancelled,
                message: Some("Too short".into()),
                preview: !insert,
                ..Default::default()
            });
            self.schedule_hide(900);
            return;
        }
        *session.shared.finish.lock() = Some(FinishInfo { duration, stopped_at: Instant::now() });
        session.shared.fate.store(FATE_FINISH, Ordering::Release);
        let total = session.shared.samples_sent.load(Ordering::Acquire).max(1);
        let done = session.shared.samples_done.load(Ordering::Acquire);
        // Dropping the subscription closes the frame channel: the collector flushes its
        // tail and the worker finishes once that last chunk is through.
        drop(session);

        self.set_state(DictationState {
            phase: Phase::Transcribing,
            duration_ms: Some(duration.as_millis() as u64),
            preview: !insert,
            progress: Some(((done * 100) / total).min(99) as u8),
            ..Default::default()
        });
    }

    pub fn cancel(self: &Arc<Self>) {
        if let Some(session) = self.session.lock().take() {
            session.shared.fate.store(FATE_DISCARD, Ordering::Release);
            drop(session);
            self.set_state(DictationState { phase: Phase::Cancelled, ..Default::default() });
            self.schedule_hide(700);
        }
    }

    // ---- internals ---------------------------------------------------------

    fn run_worker(self: Arc<Self>, session_id: u64, rx: Receiver<Vec<f32>>, shared: Arc<Shared>, insert: bool, settings: Settings) {
        let state = self.app.state::<AppState>();
        let vocab_words: Vec<String> =
            settings.vocabulary.iter().filter(|v| v.enabled).map(|v| v.word.clone()).collect();
        let opts = TranscribeOptions {
            language: settings.model.language.clone(),
            translate: settings.model.translate_to_english,
            initial_prompt: vocabulary_prompt(&vocab_words),
            decoding: settings.model.decoding,
            threads: settings.model.threads,
            bias_words: vocab_words.clone(),
            bias_strength: settings.model.vocabulary_boost.strength(),
        };
        let processor = PostProcessor::new(&settings.dictation, &settings.vocabulary);
        let mut parts: Vec<String> = Vec::new();
        let mut error: Option<anyhow::Error> = None;
        let mut inference = Duration::ZERO;

        while let Ok(chunk) = rx.recv() {
            let n = chunk.len() as u64;
            if error.is_none() && shared.fate.load(Ordering::Acquire) != FATE_DISCARD {
                let ratio = audio::speech_ratio(&chunk);
                // skip chunks that are silence end to end
                if ratio >= 0.01 {
                    let t0 = Instant::now();
                    match state.engine.transcribe(&chunk, &opts) {
                        Ok(t) => {
                            let text = processor.process(&t.text, ratio < 0.04);
                            if !text.is_empty() {
                                parts.push(text);
                            }
                        }
                        Err(e) => error = Some(e),
                    }
                    inference += t0.elapsed();
                }
            }
            let done = shared.samples_done.fetch_add(n, Ordering::AcqRel) + n;
            if shared.fate.load(Ordering::Acquire) == FATE_FINISH
                && self.latest_session.load(Ordering::Relaxed) == session_id
            {
                let total = shared.samples_sent.load(Ordering::Acquire).max(1);
                let mut st = self.state.lock().clone();
                if st.phase == Phase::Transcribing {
                    st.progress = Some(((done * 100) / total).min(99) as u8);
                    self.set_state(st);
                }
            }
        }

        // The channel only closes after the collector flushed, which happens after
        // stop()/cancel() decided the fate; if the microphone died early, wait for it.
        loop {
            match shared.fate.load(Ordering::Acquire) {
                FATE_PENDING => std::thread::sleep(Duration::from_millis(50)),
                FATE_FINISH => break,
                _ => return,
            }
        }
        let Some(info) = shared.finish.lock().take() else { return };
        self.finish(session_id, parts, error, inference, info, insert, &settings);
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        self: &Arc<Self>,
        session_id: u64,
        parts: Vec<String>,
        error: Option<anyhow::Error>,
        inference: Duration,
        info: FinishInfo,
        insert: bool,
        settings: &Settings,
    ) {
        let state = self.app.state::<AppState>();
        let duration = info.duration;
        let is_latest = self.latest_session.load(Ordering::Relaxed) == session_id;
        let waited = || info.stopped_at.elapsed().as_millis() as u64;

        if let Some(e) = &error {
            log::error!("transcription failed after {} chunk(s): {e:#}", parts.len());
        }
        if parts.is_empty() {
            if let Some(e) = error {
                if settings.general.play_sounds {
                    sounds::play(sounds::Cue::Error);
                }
                if is_latest {
                    self.set_state(DictationState {
                        phase: Phase::Error,
                        message: Some(format!("{e:#}")),
                        preview: !insert,
                        ..Default::default()
                    });
                    self.schedule_hide(2500);
                }
            } else if is_latest {
                self.set_state(DictationState {
                    phase: Phase::Cancelled,
                    message: Some("Nothing heard".into()),
                    duration_ms: Some(duration.as_millis() as u64),
                    latency_ms: Some(waited()),
                    preview: !insert,
                    ..Default::default()
                });
                self.schedule_hide(900);
            }
            return;
        }

        // One more pass over the joined text so sentence starts across chunk
        // boundaries get their capitals; the per-chunk pass already did the rest.
        let processor = PostProcessor::new(&settings.dictation, &settings.vocabulary);
        let mut text = processor.process(&parts.join(" "), false);
        if text.is_empty() {
            if is_latest {
                self.set_state(DictationState {
                    phase: Phase::Cancelled,
                    message: Some("Nothing heard".into()),
                    duration_ms: Some(duration.as_millis() as u64),
                    latency_ms: Some(waited()),
                    preview: !insert,
                    ..Default::default()
                });
                self.schedule_hide(900);
            }
            return;
        }
        log::info!(
            "dictation: {:.1}s audio, {} chunk(s), {:.0}ms inference, inserted {}ms after release",
            duration.as_secs_f32(),
            parts.len(),
            inference.as_millis(),
            waited()
        );

        if insert {
            if settings.general.trailing_space {
                text.push(' ');
            }
            if let Err(e) = crate::inject::insert_text(&self.app, &text, settings.general.insert_method) {
                log::error!("insert failed: {e:#}");
                let _ = crate::inject::copy_text(&text);
                if is_latest {
                    self.set_state(DictationState {
                        phase: Phase::Error,
                        text: Some(text.trim_end().to_string()),
                        message: Some(format!("{e:#} — copied to clipboard instead")),
                        duration_ms: Some(duration.as_millis() as u64),
                        latency_ms: Some(waited()),
                        preview: false,
                        progress: None,
                    });
                    self.schedule_hide(2500);
                }
                return;
            }
        }
        let shown = text.trim_end().to_string();
        state.history.append(HistoryEntry::dictation(shown.clone(), duration.as_millis() as u64, state.engine.model_id()));
        let _ = self.app.emit("history-updated", ());
        if is_latest || !insert {
            self.set_state(DictationState {
                phase: Phase::Inserted,
                text: Some(shown),
                message: None,
                duration_ms: Some(duration.as_millis() as u64),
                latency_ms: Some(waited()),
                preview: !insert,
                progress: None,
            });
            self.schedule_hide(1400);
        }
    }

    fn flash_error(self: &Arc<Self>, message: &str) {
        let settings = self.settings();
        if settings.general.play_sounds {
            sounds::play(sounds::Cue::Error);
        }
        if settings.general.show_overlay {
            overlay::show(&self.app);
        }
        self.set_state(DictationState { phase: Phase::Error, message: Some(message.to_string()), ..Default::default() });
        self.schedule_hide(2200);
    }

    /// Hide the overlay after `ms` unless a newer session started meanwhile, then return to idle.
    fn schedule_hide(self: &Arc<Self>, ms: u64) {
        let gen = self.hide_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let this = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            if this.hide_generation.load(Ordering::Relaxed) != gen || this.is_listening() {
                return;
            }
            this.set_state(DictationState::default());
            let meeting_visible = this.app.state::<AppState>().meetings.needs_overlay();
            if !meeting_visible {
                overlay::hide(&this.app);
            }
        });
    }
}

// ---- segmenter -------------------------------------------------------------

const FRAME_MS: usize = 20;
/// Never split a recording shorter than this: ordinary dictations stay one whisper call.
const SOFT_MAX_SECS: usize = 20;
/// Past this, cut at the quietest recent frame even without a pause.
const HARD_MAX_SECS: usize = 45;
/// Once past SOFT_MAX, a pause this long is a safe place to cut.
const CUT_SILENCE_MS: usize = 500;
/// Window searched for the quietest frame on a forced cut.
const FORCED_CUT_WINDOW_MS: usize = 10_000;

/// Splits a live recording into chunks the engine can transcribe while the rest is
/// still being spoken. Never drops audio; only chooses where to cut.
struct Segmenter {
    frame: usize,
    buf: Vec<f32>,
    /// rms of every complete frame currently in `buf`
    rms: Vec<f32>,
    noise: f32,
    /// trailing run of frames below the noise threshold
    silence_frames: usize,
}

impl Segmenter {
    fn new() -> Self {
        Self {
            frame: TARGET_RATE as usize * FRAME_MS / 1000,
            buf: Vec::with_capacity(TARGET_RATE as usize * 30),
            rms: Vec::new(),
            noise: 0.004,
            silence_frames: 0,
        }
    }

    fn push(&mut self, samples: &[f32], out: &mut Vec<Vec<f32>>) {
        self.buf.extend_from_slice(samples);
        let soft = TARGET_RATE as usize * SOFT_MAX_SECS;
        let hard = TARGET_RATE as usize * HARD_MAX_SECS;
        let cut_frames = CUT_SILENCE_MS / FRAME_MS;
        while (self.rms.len() + 1) * self.frame <= self.buf.len() {
            let start = self.rms.len() * self.frame;
            let (rms, _) = audio::level(&self.buf[start..start + self.frame]);
            let threshold = (self.noise * 3.5).max(0.012);
            if rms < threshold {
                self.noise = self.noise * 0.97 + rms * 0.03;
                self.silence_frames += 1;
            } else {
                self.silence_frames = 0;
            }
            self.rms.push(rms);
            let analysed = self.rms.len() * self.frame;
            if analysed >= soft && self.silence_frames >= cut_frames {
                // cut in the middle of the pause so neither side loses speech
                let cut = analysed - (self.silence_frames / 2) * self.frame;
                self.cut_at(cut, out);
            } else if analysed >= hard {
                // no pause in sight: cut at the quietest frame of the recent window
                let window = (FORCED_CUT_WINDOW_MS / FRAME_MS).min(self.rms.len());
                let first = self.rms.len() - window;
                let quietest = (first..self.rms.len())
                    .min_by(|&a, &b| self.rms[a].partial_cmp(&self.rms[b]).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(self.rms.len() - 1);
                self.cut_at((quietest + 1) * self.frame, out);
            }
        }
    }

    /// Emit `buf[..cut]` (frame aligned) and keep the rest.
    fn cut_at(&mut self, cut: usize, out: &mut Vec<Vec<f32>>) {
        let cut = cut.min(self.buf.len());
        if cut == 0 {
            return;
        }
        let rest = self.buf.split_off(cut);
        let chunk = std::mem::replace(&mut self.buf, rest);
        out.push(chunk);
        let cut_frames = (cut / self.frame).min(self.rms.len());
        self.rms.drain(..cut_frames);
        self.silence_frames = self.silence_frames.min(self.rms.len());
    }

    fn flush(&mut self, out: &mut Vec<Vec<f32>>) {
        if !self.buf.is_empty() {
            out.push(std::mem::take(&mut self.buf));
        }
        self.rms.clear();
        self.silence_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        let n = (TARGET_RATE as f32 * secs) as usize;
        (0..n).map(|i| amp * (i as f32 * 0.3).sin()).collect()
    }

    fn feed(seg: &mut Segmenter, audio: &[f32], out: &mut Vec<Vec<f32>>) {
        for frame in audio.chunks(480) {
            seg.push(frame, out);
        }
    }

    #[test]
    fn short_recordings_stay_whole() {
        let mut seg = Segmenter::new();
        let mut out = Vec::new();
        let mut audio = tone(8.0, 0.2);
        audio.extend(tone(2.0, 0.0)); // a long pause, but we are under SOFT_MAX
        audio.extend(tone(5.0, 0.2));
        feed(&mut seg, &audio, &mut out);
        assert!(out.is_empty());
        seg.flush(&mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), audio.len(), "no sample lost");
    }

    #[test]
    fn long_recordings_cut_inside_pauses() {
        let mut seg = Segmenter::new();
        let mut out = Vec::new();
        let mut audio = tone(22.0, 0.2);
        audio.extend(tone(1.0, 0.0)); // pause after SOFT_MAX → cut here
        audio.extend(tone(10.0, 0.2));
        feed(&mut seg, &audio, &mut out);
        assert_eq!(out.len(), 1);
        let first = out[0].len() as f32 / TARGET_RATE as f32;
        assert!(first > 22.0 && first < 23.0, "cut inside the pause, got {first}s");
        seg.flush(&mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out.iter().map(Vec::len).sum::<usize>(), audio.len(), "no sample lost");
    }

    #[test]
    fn forced_cut_without_pauses() {
        let mut seg = Segmenter::new();
        let mut out = Vec::new();
        let audio = tone(70.0, 0.2); // never quiet
        feed(&mut seg, &audio, &mut out);
        seg.flush(&mut out);
        assert!(out.len() >= 2);
        assert!(out.iter().all(|c| c.len() <= TARGET_RATE as usize * HARD_MAX_SECS));
        assert_eq!(out.iter().map(Vec::len).sum::<usize>(), audio.len(), "no sample lost");
    }
}
