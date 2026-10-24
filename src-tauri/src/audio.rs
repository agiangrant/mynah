//! Microphone / loopback capture. Each device runs on its own thread that owns the
//! cpal stream and fans out 16 kHz mono f32 frames to subscribers. Streams stay
//! warm for a configurable period after the last subscriber leaves.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const TARGET_RATE: u32 = 16_000;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
    pub kind: &'static str,
}

pub fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|desc| desc.name().to_string())
}

pub fn list_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_in = host.default_input_device().and_then(|d| device_name(&d));
    let default_out = host.default_output_device().and_then(|d| device_name(&d));
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Some(name) = device_name(&d) {
                out.push(AudioDevice { is_default: Some(&name) == default_in.as_ref(), name, kind: "input" });
            }
        }
    }
    if let Ok(devices) = host.output_devices() {
        for d in devices {
            if let Some(name) = device_name(&d) {
                if out.iter().any(|x| x.name == name) {
                    continue;
                }
                out.push(AudioDevice { is_default: Some(&name) == default_out.as_ref(), name, kind: "output" });
            }
        }
    }
    out
}

/// Windowed-sinc resampler (band-limited interpolation). Good enough for ASR, no deps.
pub struct Resampler {
    step: f64,
    pos: f64,
    buf: Vec<f32>,
    half: usize,
    cutoff: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let half = 16usize;
        let cutoff = (out_rate.min(in_rate) as f64 / in_rate as f64) * 0.5 * 0.92;
        Self { step: in_rate as f64 / out_rate as f64, pos: (half - 1) as f64, buf: vec![0.0; half - 1], half, cutoff }
    }

    #[inline]
    fn kernel(&self, x: f64) -> f32 {
        if x.abs() < 1e-9 {
            return (2.0 * self.cutoff) as f32;
        }
        let px = std::f64::consts::PI * x;
        let sinc = (2.0 * self.cutoff * px).sin() / px;
        // Hann window over [-half, half]
        let w = 0.5 * (1.0 + (px / self.half as f64).cos());
        (sinc * w) as f32
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.step - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        let half = self.half;
        loop {
            let i = self.pos.floor() as usize;
            if i + half >= self.buf.len() {
                break;
            }
            let frac = self.pos - i as f64;
            let mut acc = 0.0f32;
            // taps from i-half+1 ..= i+half
            let start = i + 1 - half;
            for (k, sample) in self.buf[start..=i + half].iter().enumerate() {
                let x = (k as f64) - (half as f64 - 1.0) - frac;
                acc += sample * self.kernel(x);
            }
            out.push(acc.clamp(-1.0, 1.0));
            self.pos += self.step;
        }
        // drop consumed samples, keeping `half` of history
        let consumed = (self.pos.floor() as usize).saturating_sub(half);
        if consumed > 0 {
            self.buf.drain(0..consumed);
            self.pos -= consumed as f64;
        }
    }
}

pub struct Subscription {
    id: u64,
    device_key: String,
    pub rx: Receiver<Vec<f32>>,
    hub: Arc<HubInner>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.hub.unsubscribe(&self.device_key, self.id);
    }
}

type SubList = Arc<Mutex<Vec<(u64, Sender<Vec<f32>>)>>>;

enum WorkerCmd {
    Add(u64, Sender<Vec<f32>>),
    Remove(u64),
    SetWarm(Duration),
    Shutdown,
}

struct Worker {
    tx: Sender<WorkerCmd>,
}

struct HubInner {
    workers: Mutex<HashMap<String, Worker>>,
    next_id: AtomicU64,
    keep_warm: Mutex<Duration>,
}

impl HubInner {
    fn unsubscribe(&self, key: &str, id: u64) {
        if let Some(w) = self.workers.lock().get(key) {
            let _ = w.tx.send(WorkerCmd::Remove(id));
        }
    }
}

#[derive(Clone)]
pub struct AudioHub {
    inner: Arc<HubInner>,
}

impl AudioHub {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(HubInner {
                workers: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                keep_warm: Mutex::new(Duration::from_secs(30)),
            }),
        }
    }

    pub fn set_keep_warm(&self, d: Duration) {
        *self.inner.keep_warm.lock() = d;
        for w in self.inner.workers.lock().values() {
            let _ = w.tx.send(WorkerCmd::SetWarm(d));
        }
    }

    /// Subscribe to a device (None = default input). Frames are 16 kHz mono.
    pub fn subscribe(&self, device: Option<&str>) -> anyhow::Result<Subscription> {
        let key = device.unwrap_or("").to_string();
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = crossbeam_channel::bounded::<Vec<f32>>(256);

        let mut workers = self.inner.workers.lock();
        if let Some(w) = workers.get(&key) {
            if w.tx.send(WorkerCmd::Add(id, tx.clone())).is_ok() {
                return Ok(Subscription { id, device_key: key, rx, hub: self.inner.clone() });
            }
            workers.remove(&key);
        }
        let worker = spawn_worker(key.clone(), *self.inner.keep_warm.lock(), self.inner.clone())?;
        worker.tx.send(WorkerCmd::Add(id, tx)).ok();
        workers.insert(key.clone(), worker);
        Ok(Subscription { id, device_key: key, rx, hub: self.inner.clone() })
    }

    /// True while any capture stream is open (including the keep-warm period after use).
    pub fn has_open_streams(&self) -> bool {
        !self.inner.workers.lock().is_empty()
    }

    pub fn shutdown(&self) {
        for (_, w) in self.inner.workers.lock().drain() {
            let _ = w.tx.send(WorkerCmd::Shutdown);
        }
    }
}

fn find_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    if name.is_empty() {
        return host.default_input_device();
    }
    let matches = |d: &cpal::Device| device_name(d).as_deref() == Some(name);
    if let Some(d) = host.input_devices().ok().and_then(|mut it| it.find(matches)) {
        return Some(d);
    }
    // Output devices are valid capture sources for WASAPI loopback on Windows.
    if let Some(d) = host.output_devices().ok().and_then(|mut it| it.find(matches)) {
        return Some(d);
    }
    log::warn!("audio device '{name}' not found, falling back to default input");
    host.default_input_device()
}

fn spawn_worker(key: String, keep_warm: Duration, hub: Arc<HubInner>) -> anyhow::Result<Worker> {
    let (tx, rx) = crossbeam_channel::unbounded::<WorkerCmd>();
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<anyhow::Result<()>>(1);
    let thread_key = key.clone();
    std::thread::Builder::new()
        .name(format!("audio:{}", if key.is_empty() { "default" } else { &key }))
        .spawn(move || {
            let subs: SubList = Arc::new(Mutex::new(Vec::new()));
            let stream = match build_stream(&thread_key, subs.clone()) {
                Ok(s) => {
                    let _ = ready_tx.send(Ok(()));
                    s
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let mut keep_warm = keep_warm;
            let mut idle_since: Option<std::time::Instant> = None;
            loop {
                let timeout = if subs.lock().is_empty() { Duration::from_millis(500) } else { Duration::from_secs(3600) };
                match rx.recv_timeout(timeout) {
                    Ok(WorkerCmd::Add(id, tx)) => {
                        subs.lock().push((id, tx));
                        idle_since = None;
                    }
                    Ok(WorkerCmd::Remove(id)) => {
                        let mut s = subs.lock();
                        s.retain(|(i, _)| *i != id);
                        if s.is_empty() && idle_since.is_none() {
                            idle_since = Some(std::time::Instant::now());
                        }
                    }
                    Ok(WorkerCmd::SetWarm(d)) => keep_warm = d,
                    Ok(WorkerCmd::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                }
                if subs.lock().is_empty() {
                    let since = *idle_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() >= keep_warm {
                        break;
                    }
                }
            }
            drop(stream);
            hub.workers.lock().remove(&thread_key);
            log::debug!("audio worker for '{thread_key}' stopped");
        })?;
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow::anyhow!("audio device did not start"))??;
    Ok(Worker { tx })
}

fn build_stream(name: &str, subs: SubList) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = find_device(&host, name).ok_or_else(|| anyhow::anyhow!("no input device available"))?;
    let supported = device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("device '{}' has no input config: {e}", device_name(&device).unwrap_or_default()))?;
    let config = supported.config();
    let channels = config.channels as usize;
    let in_rate = config.sample_rate;
    log::info!(
        "opening '{}' @ {} Hz, {} ch, {:?}",
        device_name(&device).unwrap_or_default(),
        in_rate,
        channels,
        supported.sample_format()
    );

    let err_fn = |e: cpal::Error| log::error!("audio stream error: {e}");
    let mut resampler = Resampler::new(in_rate, TARGET_RATE);
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    let mut resampled: Vec<f32> = Vec::with_capacity(2048);

    macro_rules! make {
        ($t:ty) => {{
            device.build_input_stream(
                config,
                move |data: &[$t], _| {
                    use cpal::Sample;
                    mono.clear();
                    for frame in data.chunks(channels) {
                        let mut acc = 0.0f32;
                        for s in frame {
                            acc += f32::from_sample(*s);
                        }
                        mono.push(acc / channels as f32);
                    }
                    resampled.clear();
                    resampler.process(&mono, &mut resampled);
                    if resampled.is_empty() {
                        return;
                    }
                    let subs = subs.lock();
                    for (_, tx) in subs.iter() {
                        let _ = tx.try_send(resampled.clone());
                    }
                },
                err_fn,
                None,
            )?
        }};
    }

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => make!(f32),
        cpal::SampleFormat::I16 => make!(i16),
        cpal::SampleFormat::U16 => make!(u16),
        cpal::SampleFormat::I32 => make!(i32),
        cpal::SampleFormat::U8 => make!(u8),
        cpal::SampleFormat::I8 => make!(i8),
        cpal::SampleFormat::F64 => make!(f64),
        other => anyhow::bail!("unsupported sample format {other:?}"),
    };
    stream.play()?;
    Ok(stream)
}

/// RMS + peak of a frame.
pub fn level(frame: &[f32]) -> (f32, f32) {
    if frame.is_empty() {
        return (0.0, 0.0);
    }
    let mut sum = 0.0f32;
    let mut peak = 0.0f32;
    for s in frame {
        sum += s * s;
        peak = peak.max(s.abs());
    }
    ((sum / frame.len() as f32).sqrt(), peak)
}

/// Fraction of 20 ms frames whose RMS exceeds a speech threshold.
pub fn speech_ratio(audio: &[f32]) -> f32 {
    let frame = (TARGET_RATE / 50) as usize;
    if audio.len() < frame {
        return 0.0;
    }
    let mut frames = 0usize;
    let mut loud = 0usize;
    for chunk in audio.chunks(frame) {
        frames += 1;
        if level(chunk).0 > 0.01 {
            loud += 1;
        }
    }
    loud as f32 / frames.max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_preserves_tone() {
        let in_rate = 48_000;
        let mut r = Resampler::new(in_rate, 16_000);
        let n = in_rate as usize;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / in_rate as f32).sin() * 0.5)
            .collect();
        let mut out = Vec::new();
        r.process(&input, &mut out);
        assert!((out.len() as i64 - 16_000).abs() < 40, "got {} samples", out.len());
        let (rms, _) = level(&out[200..15_000]);
        assert!((rms - 0.3535).abs() < 0.03, "rms {rms}");
    }
}
