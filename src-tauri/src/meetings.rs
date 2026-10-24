//! Meeting detection (running meeting apps + microphone in use) and continuous
//! meeting transcription with a simple energy VAD chunker.

use crate::audio::{self, AudioHub, Subscription, TARGET_RATE};
use crate::engine::whisper::vocabulary_prompt;
use crate::engine::TranscribeOptions;
use crate::history::{HistoryEntry, HistoryKind, MeetingSegment, SegmentSource};
use crate::postprocess::PostProcessor;
use crate::settings::{SaveFormat, Settings};
use crate::state::AppState;
use crate::overlay;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct MeetingState {
    pub active: bool,
    pub app: Option<String>,
    pub started_at: Option<String>,
    pub segments: Vec<MeetingSegment>,
    pub entry_id: Option<String>,
    pub pending_app: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetected {
    pub app: String,
    pub process: String,
}

struct Chunk {
    source: SegmentSource,
    start_ms: u64,
    audio: Vec<f32>,
}

/// Energy-based voice activity chunker.
struct Chunker {
    source: SegmentSource,
    session_start: Instant,
    noise: f32,
    in_speech: bool,
    speech_frames: usize,
    silence_frames: usize,
    pre_roll: Vec<f32>,
    chunk: Vec<f32>,
    chunk_start_ms: u64,
    frame: usize,
}

const FRAME_MS: usize = 20;
const PRE_ROLL_MS: usize = 300;
const END_SILENCE_MS: usize = 800;
const MIN_SPEECH_MS: usize = 400;
const MAX_CHUNK_SECS: usize = 28;

impl Chunker {
    fn new(source: SegmentSource, session_start: Instant) -> Self {
        Self {
            source,
            session_start,
            noise: 0.004,
            in_speech: false,
            speech_frames: 0,
            silence_frames: 0,
            pre_roll: Vec::new(),
            chunk: Vec::new(),
            chunk_start_ms: 0,
            frame: TARGET_RATE as usize * FRAME_MS / 1000,
        }
    }

    fn push(&mut self, samples: &[f32], out: &mut Vec<Chunk>) {
        let frame = self.frame;
        let mut pending: Vec<f32> = std::mem::take(&mut self.pre_roll);
        // pre_roll doubles as the carry buffer for partial frames while idle
        pending.extend_from_slice(samples);
        let pre_roll_len = TARGET_RATE as usize * PRE_ROLL_MS / 1000;
        let mut i = 0;
        while i + frame <= pending.len() {
            let f = &pending[i..i + frame];
            i += frame;
            let (rms, _) = audio::level(f);
            let threshold = (self.noise * 3.5).max(0.012);
            if rms < threshold {
                self.noise = self.noise * 0.97 + rms * 0.03;
            }
            let voiced = rms >= threshold;
            if self.in_speech {
                self.chunk.extend_from_slice(f);
                if voiced {
                    self.speech_frames += 1;
                    self.silence_frames = 0;
                } else {
                    self.silence_frames += 1;
                }
                let too_long = self.chunk.len() >= TARGET_RATE as usize * MAX_CHUNK_SECS;
                if self.silence_frames * FRAME_MS >= END_SILENCE_MS || too_long {
                    self.in_speech = false;
                    let audio = std::mem::take(&mut self.chunk);
                    if self.speech_frames * FRAME_MS >= MIN_SPEECH_MS {
                        out.push(Chunk { source: self.source, start_ms: self.chunk_start_ms, audio });
                    }
                    self.speech_frames = 0;
                    self.silence_frames = 0;
                }
            } else if voiced {
                self.in_speech = true;
                self.speech_frames = 1;
                self.silence_frames = 0;
                let elapsed = self.session_start.elapsed().as_millis() as u64;
                let pre_samples = pending[..i - frame].len().min(pre_roll_len);
                self.chunk_start_ms = elapsed.saturating_sub(((pre_samples + frame) * 1000 / TARGET_RATE as usize) as u64);
                self.chunk.clear();
                let start = i - frame - pre_samples;
                self.chunk.extend_from_slice(&pending[start..i]);
            }
        }
        // keep the tail (partial frame + pre-roll window) for next time
        let keep_from = if self.in_speech { i } else { i.saturating_sub(pre_roll_len) };
        self.pre_roll = pending[keep_from.min(pending.len())..].to_vec();
        if !self.in_speech && self.pre_roll.len() > pre_roll_len + frame {
            let excess = self.pre_roll.len() - (pre_roll_len + frame);
            self.pre_roll.drain(0..excess);
        }
    }

    fn flush(&mut self, out: &mut Vec<Chunk>) {
        if self.in_speech && self.speech_frames * FRAME_MS >= MIN_SPEECH_MS {
            let audio = std::mem::take(&mut self.chunk);
            out.push(Chunk { source: self.source, start_ms: self.chunk_start_ms, audio });
        }
        self.in_speech = false;
        self.chunk.clear();
    }
}

struct Session {
    app: Option<String>,
    started: Instant,
    started_at: String,
    entry_id: String,
    segments: Vec<MeetingSegment>,
    _subs: Vec<Subscription>,
    stop_flag: Arc<AtomicBool>,
    chunk_tx: crossbeam_channel::Sender<Chunk>,
    worker: Option<std::thread::JoinHandle<()>>,
    collectors: Vec<std::thread::JoinHandle<()>>,
    auto_started: bool,
    /// last time the transcript was written to disk / history
    last_saved: Instant,
}

/// Everything needed to persist a meeting, taken while holding the session lock.
struct Snapshot {
    app: Option<String>,
    started_at: String,
    entry_id: String,
    duration_ms: u64,
    segments: Vec<MeetingSegment>,
}

impl Session {
    fn snapshot(&self) -> Snapshot {
        let mut segments = self.segments.clone();
        segments.sort_by_key(|x| x.start_ms);
        Snapshot {
            app: self.app.clone(),
            started_at: self.started_at.clone(),
            entry_id: self.entry_id.clone(),
            duration_ms: self.started.elapsed().as_millis() as u64,
            segments,
        }
    }
}

/// Transcripts are written progressively so a crash or forced quit loses at most this much.
const AUTOSAVE_EVERY: Duration = Duration::from_secs(20);

pub struct Meetings {
    app: AppHandle,
    hub: AudioHub,
    session: Mutex<Option<Session>>,
    pending_app: Mutex<Option<String>>,
    dismissed: Mutex<HashMap<String, Instant>>,
    last_seen_app: Mutex<Option<Instant>>,
}

impl Meetings {
    pub fn new(app: AppHandle, hub: AudioHub) -> Arc<Self> {
        Arc::new(Self {
            app,
            hub,
            session: Mutex::new(None),
            pending_app: Mutex::new(None),
            dismissed: Mutex::new(HashMap::new()),
            last_seen_app: Mutex::new(None),
        })
    }

    pub fn state(&self) -> MeetingState {
        let session = self.session.lock();
        match session.as_ref() {
            Some(s) => MeetingState {
                active: true,
                app: s.app.clone(),
                started_at: Some(s.started_at.clone()),
                segments: s.segments.clone(),
                entry_id: Some(s.entry_id.clone()),
                pending_app: None,
            },
            None => MeetingState {
                active: false,
                app: None,
                started_at: None,
                segments: vec![],
                entry_id: None,
                pending_app: self.pending_app.lock().clone(),
            },
        }
    }

    pub fn is_active(&self) -> bool {
        self.session.lock().is_some()
    }

    /// The overlay should stay visible while a meeting is recording or a prompt is pending.
    pub fn needs_overlay(&self) -> bool {
        self.is_active() || self.pending_app.lock().is_some()
    }

    fn emit_state(&self) {
        let _ = self.app.emit("meeting-state", self.state());
    }

    fn settings(&self) -> Settings {
        self.app.state::<AppState>().settings.get()
    }

    pub fn toggle(self: &Arc<Self>) {
        if self.is_active() {
            self.stop();
        } else {
            let app = self.pending_app.lock().clone();
            self.start(app, false);
        }
    }

    pub fn dismiss_prompt(&self) {
        if let Some(app) = self.pending_app.lock().take() {
            self.dismissed.lock().insert(app, Instant::now());
        }
        self.emit_state();
        if !self.app.state::<AppState>().dictation.is_listening() {
            overlay::hide(&self.app);
        }
    }

    pub fn start(self: &Arc<Self>, app_name: Option<String>, auto_started: bool) {
        let settings = self.settings();
        if !self.app.state::<AppState>().engine.is_ready() {
            log::warn!("meeting transcription requested without a loaded model");
            return;
        }
        let mut session = self.session.lock();
        if session.is_some() {
            return;
        }
        *self.pending_app.lock() = None;
        if let Some(a) = &app_name {
            self.dismissed.lock().remove(a);
        }

        let started = Instant::now();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (chunk_tx, chunk_rx) = crossbeam_channel::unbounded::<Chunk>();
        let mut subs = Vec::new();
        let mut collectors = Vec::new();

        let mut sources: Vec<(SegmentSource, Option<String>)> = vec![(SegmentSource::Mic, settings.general.input_device.clone())];
        if settings.meetings.include_system_audio {
            if let Some(dev) = settings.meetings.system_audio_device.clone() {
                sources.push((SegmentSource::System, Some(dev)));
            }
        }
        for (source, device) in sources {
            match self.hub.subscribe(device.as_deref()) {
                Ok(sub) => {
                    let rx = sub.rx.clone();
                    let tx = chunk_tx.clone();
                    let flag = stop_flag.clone();
                    let handle = std::thread::Builder::new()
                        .name(format!("meeting-{source:?}"))
                        .spawn(move || {
                            let mut chunker = Chunker::new(source, started);
                            let mut out = Vec::new();
                            while let Ok(frame) = rx.recv() {
                                if flag.load(Ordering::Relaxed) {
                                    break;
                                }
                                chunker.push(&frame, &mut out);
                                for c in out.drain(..) {
                                    let _ = tx.send(c);
                                }
                            }
                            chunker.flush(&mut out);
                            for c in out.drain(..) {
                                let _ = tx.send(c);
                            }
                        })
                        .expect("spawn meeting collector");
                    subs.push(sub);
                    collectors.push(handle);
                }
                Err(e) => log::error!("meeting audio source failed ({source:?}): {e:#}"),
            }
        }
        if subs.is_empty() {
            log::error!("no audio source for meeting transcription");
            return;
        }

        // transcription worker
        let this = Arc::downgrade(self);
        let app_handle = self.app.clone();
        let worker = std::thread::Builder::new()
            .name("meeting-transcribe".into())
            .spawn(move || {
                let state = app_handle.state::<AppState>();
                while let Ok(chunk) = chunk_rx.recv() {
                    let settings = state.settings.get();
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
                    let duration_ms = (chunk.audio.len() * 1000 / TARGET_RATE as usize) as u64;
                    let low_energy = audio::speech_ratio(&chunk.audio) < 0.04;
                    match state.engine.transcribe(&chunk.audio, &opts) {
                        Ok(t) => {
                            let text = PostProcessor::new(&settings.dictation, &settings.vocabulary).process(&t.text, low_energy);
                            if text.is_empty() {
                                continue;
                            }
                            let seg = MeetingSegment {
                                start_ms: chunk.start_ms,
                                end_ms: chunk.start_ms + duration_ms,
                                text,
                                source: chunk.source,
                            };
                            if let Some(m) = this.upgrade() {
                                let mut snapshot = None;
                                if let Some(s) = m.session.lock().as_mut() {
                                    s.segments.push(seg.clone());
                                    if s.last_saved.elapsed() >= AUTOSAVE_EVERY {
                                        s.last_saved = Instant::now();
                                        snapshot = Some(s.snapshot());
                                    }
                                }
                                let _ = app_handle.emit("meeting-segment", seg);
                                if let Some(snap) = snapshot {
                                    m.persist(&settings, &snap);
                                }
                            }
                        }
                        Err(e) => log::error!("meeting chunk failed: {e:#}"),
                    }
                }
            })
            .expect("spawn meeting worker");

        *session = Some(Session {
            app: app_name,
            started,
            started_at: chrono::Local::now().to_rfc3339(),
            entry_id: uuid::Uuid::new_v4().to_string(),
            segments: Vec::new(),
            _subs: subs,
            stop_flag,
            chunk_tx,
            worker: Some(worker),
            collectors,
            auto_started,
            last_saved: Instant::now(),
        });
        *self.last_seen_app.lock() = Some(Instant::now());
        drop(session);
        self.emit_state();
        if settings.general.show_overlay {
            overlay::show(&self.app);
        }
        self.app.state::<AppState>().refresh_tray();
    }

    pub fn stop(self: &Arc<Self>) {
        let Some(mut session) = self.session.lock().take() else { return };
        let settings = self.settings();
        session.stop_flag.store(true, Ordering::Relaxed);
        // Dropping subscriptions closes the frame channels → collectors flush and exit.
        session._subs.clear();
        for c in session.collectors.drain(..) {
            let _ = c.join();
        }
        // Close the chunk channel so the worker drains and exits.
        let (dummy_tx, _) = crossbeam_channel::unbounded::<Chunk>();
        let tx = std::mem::replace(&mut session.chunk_tx, dummy_tx);
        drop(tx);
        if let Some(w) = session.worker.take() {
            let _ = w.join();
        }

        let snap = session.snapshot();
        if !snap.segments.is_empty() {
            self.persist(&settings, &snap);
        }
        let state = self.app.state::<AppState>();
        self.emit_state();
        if !state.dictation.is_listening() {
            overlay::hide(&self.app);
        }
        state.refresh_tray();
    }

    /// Write the transcript file and the history entry for the current state of a
    /// meeting. Called every AUTOSAVE_EVERY while it runs and once more at the end;
    /// the file name is derived from the start time, so each call overwrites the last.
    fn persist(&self, settings: &Settings, snap: &Snapshot) {
        let saved_path = match self.save_transcript(settings, &snap.segments, snap.app.as_deref(), &snap.started_at) {
            Ok(p) => Some(p.to_string_lossy().to_string()),
            Err(e) => {
                log::error!("saving meeting transcript failed: {e:#}");
                None
            }
        };
        let text = render_transcript(&snap.segments, SaveFormat::Txt, snap.app.as_deref(), &snap.started_at);
        let started = chrono::DateTime::parse_from_rfc3339(&snap.started_at)
            .map(|d| d.with_timezone(&chrono::Local))
            .unwrap_or_else(|_| chrono::Local::now());
        let title = format!(
            "{} · {}",
            snap.app.clone().unwrap_or_else(|| "Meeting".into()),
            started.format("%b %-d, %H:%M")
        );
        let state = self.app.state::<AppState>();
        state.history.upsert(HistoryEntry {
            id: snap.entry_id.clone(),
            kind: HistoryKind::Meeting,
            created_at: snap.started_at.clone(),
            text,
            duration_ms: snap.duration_ms,
            model_id: state.engine.model_id(),
            title: Some(title),
            app: snap.app.clone(),
            segments: Some(snap.segments.clone()),
            saved_path,
        });
        let _ = self.app.emit("history-updated", ());
    }

    fn save_transcript(
        &self,
        settings: &Settings,
        segments: &[MeetingSegment],
        app: Option<&str>,
        started_at: &str,
    ) -> anyhow::Result<PathBuf> {
        let dir = match &settings.meetings.save_dir {
            Some(d) if !d.trim().is_empty() => PathBuf::from(d),
            _ => self.app.state::<AppState>().data_dir.join("meetings"),
        };
        std::fs::create_dir_all(&dir)?;
        let ext = match settings.meetings.save_format {
            SaveFormat::Md => "md",
            SaveFormat::Txt => "txt",
        };
        // Stable per meeting so progressive saves overwrite the same file.
        let stamp = chrono::DateTime::parse_from_rfc3339(started_at)
            .map(|d| d.with_timezone(&chrono::Local))
            .unwrap_or_else(|_| chrono::Local::now())
            .format("%Y-%m-%d %H-%M");
        let safe_app: String = app
            .unwrap_or("Meeting")
            .chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{stamp} {safe_app}.{ext}"));
        std::fs::write(&path, render_transcript(segments, settings.meetings.save_format, app, started_at))?;
        Ok(path)
    }

    // ---- detection ---------------------------------------------------------

    /// Called every few seconds from the detector task.
    pub fn tick(self: &Arc<Self>) {
        let settings = self.settings();
        if !settings.meetings.detection_enabled {
            return;
        }
        let detected = detect_meeting_app(&settings.meetings.apps);
        let state = self.app.state::<AppState>();
        let present = match &detected {
            None => false,
            Some(_) if !settings.meetings.require_mic_in_use => true,
            // While we record (meeting or dictation) the mic is ours; only the process matters.
            Some(_) if self.is_active() || state.dictation.is_listening() => true,
            // Our own warm mic stream would read as "in use"; wait for it to close before judging.
            Some(_) if state.hub.has_open_streams() => false,
            // Unknown on this platform → trust the process match.
            Some(_) => mic_in_use().unwrap_or(true),
        };
        let dictating = state.dictation.is_listening();

        if present {
            *self.last_seen_app.lock() = Some(Instant::now());
        }

        if self.is_active() {
            // auto-stop once the app is gone for a while
            let auto = self.session.lock().as_ref().map(|s| s.auto_started).unwrap_or(false);
            if auto && !present {
                let idle = self.last_seen_app.lock().map(|t| t.elapsed()).unwrap_or_default();
                if idle >= Duration::from_secs(settings.meetings.end_after_idle_secs as u64) {
                    log::info!("meeting app gone; stopping transcription");
                    self.stop();
                }
            }
            return;
        }

        let Some((app_name, process)) = detected else {
            // app closed → forget dismissals and pending prompts
            if self.pending_app.lock().take().is_some() {
                self.emit_state();
                if !dictating {
                    overlay::hide(&self.app);
                }
            }
            self.dismissed.lock().clear();
            return;
        };
        if !present {
            return;
        }
        if self.dismissed.lock().contains_key(&app_name) {
            return;
        }
        if settings.meetings.auto_start {
            log::info!("meeting detected in {app_name}; auto-starting transcription");
            self.start(Some(app_name), true);
        } else if self.pending_app.lock().as_deref() != Some(app_name.as_str()) {
            *self.pending_app.lock() = Some(app_name.clone());
            let _ = self.app.emit("meeting-detected", MeetingDetected { app: app_name, process });
            self.emit_state();
            if settings.general.show_overlay {
                overlay::show(&self.app);
            }
        }
    }
}

fn render_transcript(segments: &[MeetingSegment], format: SaveFormat, app: Option<&str>, started_at: &str) -> String {
    let mut out = String::new();
    let title = format!("{} transcript", app.unwrap_or("Meeting"));
    match format {
        SaveFormat::Md => {
            out.push_str(&format!("# {title}\n\n_{started_at}_\n\n"));
        }
        SaveFormat::Txt => {
            out.push_str(&format!("{title}\n{started_at}\n\n"));
        }
    }
    for s in segments {
        let secs = s.start_ms / 1000;
        let stamp = format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60);
        let who = match s.source {
            SegmentSource::Mic => "You",
            SegmentSource::System => "Others",
        };
        match format {
            SaveFormat::Md => out.push_str(&format!("**[{stamp}] {who}:** {}\n\n", s.text)),
            SaveFormat::Txt => out.push_str(&format!("[{stamp}] {who}: {}\n", s.text)),
        }
    }
    out
}

/// Returns (display name, process name) of the first running meeting app.
/// Matching is exact on the process name (case-insensitive, `.exe` ignored) so helper
/// daemons such as `facetimemessagestored` or `Slack Helper` do not count. A trailing `*`
/// in a pattern allows a prefix match.
fn detect_meeting_app(apps: &[String]) -> Option<(String, String)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let own = std::process::id();
    let mut best: Option<(usize, String, String)> = None;
    for (pid, proc_) in sys.processes() {
        if pid.as_u32() == own {
            continue;
        }
        let name = proc_.name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        let lower = lower.strip_suffix(".exe").unwrap_or(&lower).to_string();
        for (idx, app) in apps.iter().enumerate() {
            let a = app.trim().to_lowercase();
            if a.is_empty() {
                continue;
            }
            let hit = match a.strip_suffix('*') {
                Some(prefix) => lower.starts_with(prefix),
                None => lower == a,
            };
            if hit && best.as_ref().map(|b| idx < b.0).unwrap_or(true) {
                best = Some((idx, app.clone(), name.clone()));
            }
        }
    }
    best.map(|(_, app, process)| (friendly_app_name(&app), process))
}

fn friendly_app_name(app: &str) -> String {
    let l = app.to_lowercase();
    if l.contains("zoom") {
        "Zoom".into()
    } else if l.contains("teams") {
        "Microsoft Teams".into()
    } else if l.contains("webex") {
        "Webex".into()
    } else if l.contains("facetime") {
        "FaceTime".into()
    } else if l.contains("slack") {
        "Slack".into()
    } else if l.contains("discord") {
        "Discord".into()
    } else {
        app.to_string()
    }
}

/// Is any process currently capturing from the default input device?
/// macOS: CoreAudio `kAudioDevicePropertyDeviceIsRunningSomewhere` (no permission needed).
/// Linux: PulseAudio/PipeWire source outputs. Windows: unknown (None).
#[cfg(target_os = "macos")]
pub fn mic_in_use() -> Option<bool> {
    use objc2_core_audio::{
        kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
        AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    };
    use std::ptr::NonNull;
    unsafe {
        let mut addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut device: AudioObjectID = 0;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        let status = AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(&mut device as *mut _ as *mut std::ffi::c_void),
        );
        if status != 0 || device == 0 {
            return None;
        }
        let mut addr = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut running: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = AudioObjectGetPropertyData(
            device,
            NonNull::from(&mut addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(&mut running as *mut _ as *mut std::ffi::c_void),
        );
        if status != 0 {
            return None;
        }
        Some(running != 0)
    }
}

#[cfg(target_os = "linux")]
pub fn mic_in_use() -> Option<bool> {
    let out = std::process::Command::new("pactl").args(["list", "short", "source-outputs"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn mic_in_use() -> Option<bool> {
    None
}
